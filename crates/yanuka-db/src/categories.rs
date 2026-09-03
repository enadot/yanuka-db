//! Smart categories (ADR-038).
//!
//! A category is a shelf: a name, a face (icon, colour, position) and,
//! optionally, a *rule* describing who belongs. Rule membership is cached in
//! `category_matches` and refreshed by the same code path that maintains the
//! search index — never by triggers, because a rule reads joined tables and
//! the Hebrew normalizer, which SQL alone cannot. Manual membership lives in
//! `contact_categories` with a mode: `include` pins a person in, `exclude`
//! keeps them out even when the rule says otherwise. The `category_members`
//! view combines the two; every reader goes through it.
//!
//! The rule vocabulary is the one `packages/core/src/category-rules.ts`
//! evaluates in TypeScript; the contract suite holds the two together.

use std::collections::{HashMap, HashSet};

use rusqlite::{params, params_from_iter, Connection, OptionalExtension, Row, Transaction};
use serde::{Deserialize, Serialize};
use serde_json::json;
use time::format_description::well_known::Rfc3339;
use time::{Duration, OffsetDateTime};
use yanuka_search::normalize_text;

use crate::error::{DbError, Result};
use crate::index::reindex_contact;
use crate::models::*;
use crate::mutation::{self, Operation};
use crate::repository::{contact_from_row, device_id, summarize};
use crate::{new_id, now_iso};

/// Cosine floor for a `meaning` condition. A little stricter than search's
/// 0.80: search has a lexical layer to anchor it, a category does not.
pub const MEANING_MIN_COSINE: f32 = 0.82;

const DEFAULTS_KEY: &str = "default_categories_installed";
const DEFAULTS_JSON: &str =
    include_str!("../../../packages/database/seeds/default-categories.json");

/// The embedding engine, when the `semantic` feature is compiled in; `()`
/// otherwise, so every signature reads the same either way.
#[cfg(feature = "semantic")]
pub type Meaning<'a> = Option<&'a crate::semantic::SemanticEngine>;
#[cfg(not(feature = "semantic"))]
pub type Meaning<'a> = Option<&'a ()>;

// ---------------------------------------------------------------------------
// The rule vocabulary
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleField {
    Name,
    Occupation,
    City,
    Country,
    Organization,
    Tag,
    Specialty,
    Notes,
    Anywhere,
    Relationship,
    Phone,
    Email,
    Created,
    Meaning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleOperator {
    Contains,
    NotContains,
    Is,
    IsNot,
    IsEmpty,
    IsNotEmpty,
    WithinDays,
    Similar,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuleMatch {
    #[default]
    All,
    Any,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleCondition {
    pub field: RuleField,
    pub op: RuleOperator,
    #[serde(default)]
    pub values: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CategoryRule {
    #[serde(rename = "match", default)]
    pub combine: RuleMatch,
    #[serde(default)]
    pub conditions: Vec<RuleCondition>,
}

impl CategoryRule {
    fn has_meaning(&self) -> bool {
        self.conditions.iter().any(|condition| condition.field == RuleField::Meaning)
    }
}

// ---------------------------------------------------------------------------
// Rule → SQL
// ---------------------------------------------------------------------------

/// One place a field's text lives: a normalized column expression on the
/// contact row (`c`), or a child table reached by an EXISTS subquery.
enum Source {
    Column(&'static str),
    Child { from: &'static str, column: &'static str },
}

fn sources(field: RuleField) -> Vec<Source> {
    use Source::*;
    match field {
        RuleField::Name => vec![
            Column("yanuka_normalize(c.display_name)"),
            Child {
                from: "SELECT 1 FROM contact_aliases a WHERE a.contact_id = c.id AND a.deleted_at IS NULL",
                column: "a.normalized",
            },
        ],
        RuleField::Occupation => vec![
            Column("yanuka_normalize(c.profession)"),
            Column("yanuka_normalize(c.role)"),
            Column("yanuka_normalize(c.title)"),
            Column("yanuka_normalize(c.prefix)"),
        ],
        RuleField::City => {
            vec![Column("yanuka_normalize(c.city)"), Column("yanuka_normalize(c.region)")]
        }
        RuleField::Country => vec![Column("upper(c.country)")],
        RuleField::Organization => vec![Child {
            from: "SELECT 1 FROM contact_organizations co JOIN organizations o ON o.id = co.organization_id
                    WHERE co.contact_id = c.id AND co.deleted_at IS NULL AND o.deleted_at IS NULL",
            column: "o.normalized",
        }],
        RuleField::Tag => vec![Child {
            from: "SELECT 1 FROM contact_tags ct JOIN tags t ON t.id = ct.tag_id
                    WHERE ct.contact_id = c.id AND ct.deleted_at IS NULL AND t.deleted_at IS NULL",
            column: "t.normalized",
        }],
        RuleField::Specialty => vec![Child {
            from: "SELECT 1 FROM contact_specialties s WHERE s.contact_id = c.id AND s.deleted_at IS NULL",
            column: "s.normalized",
        }],
        RuleField::Notes => vec![
            Column("yanuka_normalize(c.notes)"),
            Column("yanuka_normalize(c.reason_for_saving)"),
            Column("yanuka_normalize(c.introduced_by)"),
            Child {
                from: "SELECT 1 FROM notes n WHERE n.contact_id = c.id AND n.deleted_at IS NULL",
                column: "yanuka_normalize(n.body)",
            },
        ],
        RuleField::Anywhere => [
            RuleField::Name,
            RuleField::Occupation,
            RuleField::City,
            RuleField::Organization,
            RuleField::Tag,
            RuleField::Specialty,
            RuleField::Notes,
        ]
        .into_iter()
        .flat_map(sources)
        .collect(),
        RuleField::Relationship => vec![Child {
            from: "SELECT 1 FROM relationships r
                    WHERE (r.from_contact_id = c.id OR r.to_contact_id = c.id) AND r.deleted_at IS NULL",
            column: "r.type",
        }],
        RuleField::Phone => vec![Child {
            from: "SELECT 1 FROM contact_phones p WHERE p.contact_id = c.id AND p.deleted_at IS NULL",
            column: "p.raw",
        }],
        RuleField::Email => vec![Child {
            from: "SELECT 1 FROM contact_emails e WHERE e.contact_id = c.id AND e.deleted_at IS NULL",
            column: "e.address",
        }],
        RuleField::Created | RuleField::Meaning => Vec::new(),
    }
}

/// Values as the SQL will compare them: normalized text, upper-case country
/// codes, raw relationship types.
fn needles(condition: &RuleCondition) -> Vec<String> {
    condition
        .values
        .iter()
        .map(|value| match condition.field {
            RuleField::Country => value.trim().to_uppercase(),
            RuleField::Relationship => value.trim().to_string(),
            _ => normalize_text(value),
        })
        .filter(|value| !value.is_empty())
        .collect()
}

struct SqlBuilder {
    params: Vec<String>,
}

impl SqlBuilder {
    /// `column` matches any of the needles: whole value for `exact`, else at a
    /// word start (` ` + needle inside ` ` + haystack) — `רב` finds `רבנים`
    /// but not `ערב`.
    fn hit(&mut self, column: &str, values: &[String], exact: bool) -> String {
        let parts: Vec<String> = values
            .iter()
            .map(|value| {
                if exact {
                    self.params.push(value.clone());
                    format!("{column} = ?")
                } else {
                    self.params.push(format!(" {value}"));
                    format!("instr(' ' || {column}, ?) > 0")
                }
            })
            .collect();
        format!("({})", parts.join(" OR "))
    }

    fn present(&mut self, field: RuleField, values: &[String], exact: bool) -> String {
        let parts: Vec<String> = sources(field)
            .into_iter()
            .map(|source| match source {
                Source::Column(column) => self.hit(column, values, exact),
                Source::Child { from, column } => {
                    let hit = self.hit(column, values, exact);
                    format!("EXISTS ({from} AND {hit})")
                }
            })
            .collect();
        if parts.is_empty() {
            "0".to_string()
        } else {
            format!("({})", parts.join(" OR "))
        }
    }

    fn empty(&mut self, field: RuleField) -> String {
        let parts: Vec<String> = sources(field)
            .into_iter()
            .map(|source| match source {
                Source::Column(column) => format!("({column} IS NULL OR {column} = '')"),
                Source::Child { from, .. } => format!("NOT EXISTS ({from})"),
            })
            .collect();
        if parts.is_empty() {
            "1".to_string()
        } else {
            format!("({})", parts.join(" AND "))
        }
    }

    /// Returns `None` when the condition needs the embedding model and none is
    /// available; the caller decides what that means for the whole rule.
    fn condition(
        &mut self,
        condition: &RuleCondition,
        meaning: &Option<HashSet<String>>,
    ) -> Option<String> {
        let values = needles(condition);
        Some(match condition.op {
            RuleOperator::Contains | RuleOperator::Is => {
                if values.is_empty() {
                    "0".to_string()
                } else {
                    self.present(condition.field, &values, condition.op == RuleOperator::Is)
                }
            }
            RuleOperator::NotContains | RuleOperator::IsNot => {
                if values.is_empty() {
                    "1".to_string()
                } else {
                    let hit =
                        self.present(condition.field, &values, condition.op == RuleOperator::IsNot);
                    format!("NOT {hit}")
                }
            }
            RuleOperator::IsEmpty => self.empty(condition.field),
            RuleOperator::IsNotEmpty => format!("NOT {}", self.empty(condition.field)),
            RuleOperator::WithinDays => {
                let days: i64 = condition.values.first().and_then(|v| v.trim().parse().ok())?;
                let cutoff = (OffsetDateTime::now_utc() - Duration::days(days.max(0)))
                    .format(&Rfc3339)
                    .unwrap_or_default();
                self.params.push(cutoff);
                "c.created_at >= ?".to_string()
            }
            RuleOperator::Similar => {
                let ids = meaning.as_ref()?;
                if ids.is_empty() {
                    "0".to_string()
                } else {
                    let marks = vec!["?"; ids.len()].join(", ");
                    self.params.extend(ids.iter().cloned());
                    format!("c.id IN ({marks})")
                }
            }
        })
    }
}

/// Compiled predicate over a `contacts c` row, or `None` when the rule cannot
/// be evaluated here (a `meaning` condition without the model).
struct Compiled {
    sql: String,
    params: Vec<String>,
}

fn compile(
    connection: &Connection,
    rule: &CategoryRule,
    meaning: Meaning<'_>,
) -> Result<Option<Compiled>> {
    if rule.conditions.is_empty() {
        return Ok(Some(Compiled { sql: "0".into(), params: Vec::new() }));
    }
    let mut builder = SqlBuilder { params: Vec::new() };
    let mut parts = Vec::with_capacity(rule.conditions.len());
    for condition in &rule.conditions {
        let similar = if condition.op == RuleOperator::Similar {
            let sentence = condition.values.first().map(String::as_str).unwrap_or_default();
            resolve_meaning(connection, meaning, sentence)?
        } else {
            None
        };
        match builder.condition(condition, &similar) {
            Some(sql) => parts.push(sql),
            None => return Ok(None),
        }
    }
    let joiner = match rule.combine {
        RuleMatch::All => " AND ",
        RuleMatch::Any => " OR ",
    };
    Ok(Some(Compiled { sql: format!("({})", parts.join(joiner)), params: builder.params }))
}

#[cfg(feature = "semantic")]
fn resolve_meaning(
    connection: &Connection,
    meaning: Meaning<'_>,
    sentence: &str,
) -> Result<Option<HashSet<String>>> {
    match meaning {
        Some(engine) => Ok(Some(crate::semantic::similar_contacts(
            connection,
            engine,
            sentence,
            MEANING_MIN_COSINE,
        )?)),
        None => Ok(None),
    }
}

#[cfg(not(feature = "semantic"))]
fn resolve_meaning(
    _connection: &Connection,
    _meaning: Meaning<'_>,
    _sentence: &str,
) -> Result<Option<HashSet<String>>> {
    Ok(None)
}

/// Every live contact the rule selects, or `None` when it cannot be evaluated.
fn select_matches(
    connection: &Connection,
    rule: &CategoryRule,
    meaning: Meaning<'_>,
) -> Result<Option<HashSet<String>>> {
    let Some(compiled) = compile(connection, rule, meaning)? else {
        return Ok(None);
    };
    let mut statement = connection.prepare(&format!(
        "SELECT c.id FROM contacts c WHERE c.deleted_at IS NULL AND {}",
        compiled.sql
    ))?;
    let rows = statement
        .query_map(params_from_iter(compiled.params.iter()), |row| row.get::<_, String>(0))?;
    Ok(Some(rows.collect::<rusqlite::Result<HashSet<_>>>()?))
}

// ---------------------------------------------------------------------------
// Rows
// ---------------------------------------------------------------------------

fn parse_rule(raw: Option<String>) -> Option<CategoryRule> {
    raw.and_then(|text| serde_json::from_str(&text).ok())
}

pub fn category_from_row(row: &Row<'_>) -> rusqlite::Result<Category> {
    Ok(Category {
        id: row.get("id")?,
        name: row.get("name")?,
        normalized: row.get("normalized")?,
        description: row.get("description")?,
        parent_id: row.get("parent_id")?,
        icon: row.get("icon")?,
        color: row.get("color")?,
        rule: parse_rule(row.get("rule")?),
        sort_order: row.get("sort_order")?,
        show_on_home: row.get::<_, i64>("show_on_home")? != 0,
    })
}

fn load(connection: &Connection, id: &str) -> Result<Category> {
    connection
        .query_row(
            "SELECT * FROM categories WHERE id = ?1 AND deleted_at IS NULL",
            params![id],
            category_from_row,
        )
        .optional()?
        .ok_or_else(|| DbError::NotFound("הקטגוריה".into()))
}

fn counts(connection: &Connection) -> Result<HashMap<String, i64>> {
    let mut statement = connection
        .prepare("SELECT category_id, COUNT(*) FROM category_members GROUP BY category_id")?;
    let rows =
        statement.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)))?;
    Ok(rows.collect::<rusqlite::Result<HashMap<_, _>>>()?)
}

fn member_ids(connection: &Connection, category_id: &str) -> Result<Vec<String>> {
    let mut statement =
        connection.prepare("SELECT contact_id FROM category_members WHERE category_id = ?1")?;
    let rows = statement.query_map(params![category_id], |row| row.get::<_, String>(0))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn rule_categories(connection: &Connection) -> Result<Vec<(String, CategoryRule)>> {
    let mut statement = connection
        .prepare("SELECT id, rule FROM categories WHERE deleted_at IS NULL AND rule IS NOT NULL")?;
    let rows = statement
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)))?;
    let mut result = Vec::new();
    for row in rows {
        let (id, raw) = row?;
        if let Some(rule) = parse_rule(raw) {
            result.push((id, rule));
        }
    }
    Ok(result)
}

fn journal(
    tx: &Transaction<'_>,
    entity_type: &str,
    entity_id: &str,
    operation: Operation,
    payload: Option<&serde_json::Value>,
    previous: Option<&serde_json::Value>,
    base_version: i64,
) -> Result<()> {
    let device = device_id(tx)?;
    mutation::record(
        tx,
        mutation::NewMutation {
            entity_type,
            entity_id,
            operation,
            payload,
            previous,
            base_version,
            device_id: &device,
        },
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Keeping the cache honest
// ---------------------------------------------------------------------------

/// Write one contact's match state for one category; true when it changed.
fn set_match(
    tx: &Transaction<'_>,
    category_id: &str,
    contact_id: &str,
    matched: bool,
) -> Result<bool> {
    let changed = if matched {
        tx.execute(
            "INSERT OR IGNORE INTO category_matches (category_id, contact_id) VALUES (?1, ?2)",
            params![category_id, contact_id],
        )?
    } else {
        tx.execute(
            "DELETE FROM category_matches WHERE category_id = ?1 AND contact_id = ?2",
            params![category_id, contact_id],
        )?
    };
    Ok(changed > 0)
}

/// Re-evaluate every rule for one contact. Called by `reindex_contact` at the
/// end of each mutating operation, inside its transaction, so the cache can
/// never lag the row it describes. Rules with a `meaning` condition are left
/// to `refresh_contact_meaning`, which has the model.
pub fn refresh_contact(tx: &Transaction<'_>, contact_id: &str) -> Result<()> {
    let live: Option<i64> = tx
        .query_row(
            "SELECT 1 FROM contacts WHERE id = ?1 AND deleted_at IS NULL",
            params![contact_id],
            |row| row.get(0),
        )
        .optional()?;
    if live.is_none() {
        tx.execute("DELETE FROM category_matches WHERE contact_id = ?1", params![contact_id])?;
        return Ok(());
    }

    for (category_id, rule) in rule_categories(tx)? {
        if rule.has_meaning() {
            continue;
        }
        let Some(compiled) = compile(tx, &rule, None)? else { continue };
        let matched = tx
            .query_row(
                &format!(
                    "SELECT 1 FROM contacts c WHERE c.id = ? AND c.deleted_at IS NULL AND {}",
                    compiled.sql
                ),
                params_from_iter(std::iter::once(contact_id.to_string()).chain(compiled.params)),
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        set_match(tx, &category_id, contact_id, matched)?;
    }
    Ok(())
}

/// Rebuild one category's matches over the whole archive — after its rule
/// changed — and reindex everyone whose membership moved. Returns how many
/// contacts changed shelf. A rule the model is needed for, evaluated without
/// it, is left untouched rather than emptied.
pub fn refresh_category(
    connection: &mut Connection,
    category_id: &str,
    meaning: Meaning<'_>,
) -> Result<usize> {
    let category = load(connection, category_id)?;
    let desired: HashSet<String> = match &category.rule {
        Some(rule) => match select_matches(connection, rule, meaning)? {
            Some(set) => set,
            None => return Ok(0),
        },
        None => HashSet::new(),
    };
    let existing: HashSet<String> = {
        let mut statement =
            connection.prepare("SELECT contact_id FROM category_matches WHERE category_id = ?1")?;
        let rows = statement.query_map(params![category_id], |row| row.get::<_, String>(0))?;
        rows.collect::<rusqlite::Result<HashSet<_>>>()?
    };

    let tx = connection.transaction()?;
    let mut affected: Vec<&String> = Vec::new();
    for contact_id in existing.difference(&desired) {
        set_match(&tx, category_id, contact_id, false)?;
        affected.push(contact_id);
    }
    for contact_id in desired.difference(&existing) {
        set_match(&tx, category_id, contact_id, true)?;
        affected.push(contact_id);
    }
    for contact_id in &affected {
        reindex_contact(&tx, contact_id)?;
    }
    tx.commit()?;
    Ok(affected.len())
}

/// Evaluate the rules that need the model for one contact — the desktop calls
/// this after that contact's vectors were refreshed.
#[cfg(feature = "semantic")]
pub fn refresh_contact_meaning(
    connection: &mut Connection,
    engine: &crate::semantic::SemanticEngine,
    contact_id: &str,
) -> Result<()> {
    let mut decisions = Vec::new();
    for (category_id, rule) in rule_categories(connection)? {
        if !rule.has_meaning() {
            continue;
        }
        if let Some(set) = select_matches(connection, &rule, Some(engine))? {
            decisions.push((category_id, set.contains(contact_id)));
        }
    }
    if decisions.is_empty() {
        return Ok(());
    }
    let tx = connection.transaction()?;
    let mut changed = false;
    for (category_id, matched) in decisions {
        changed |= set_match(&tx, &category_id, contact_id, matched)?;
    }
    if changed {
        reindex_contact(&tx, contact_id)?;
    }
    tx.commit()?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Reading
// ---------------------------------------------------------------------------

pub fn list_categories(connection: &Connection) -> Result<Vec<CategorySummary>> {
    let counts = counts(connection)?;
    let mut statement = connection
        .prepare("SELECT * FROM categories WHERE deleted_at IS NULL ORDER BY sort_order, name")?;
    let rows = statement.query_map([], category_from_row)?;
    let mut result = Vec::new();
    for row in rows {
        let category = row?;
        let count = counts.get(&category.id).copied().unwrap_or(0);
        result.push(CategorySummary { category, count });
    }
    Ok(result)
}

pub fn get_category(connection: &Connection, id: &str) -> Result<Option<CategorySummary>> {
    let Some(category) = connection
        .query_row(
            "SELECT * FROM categories WHERE id = ?1 AND deleted_at IS NULL",
            params![id],
            category_from_row,
        )
        .optional()?
    else {
        return Ok(None);
    };
    let count: i64 = connection.query_row(
        "SELECT COUNT(*) FROM category_members WHERE category_id = ?1",
        params![id],
        |row| row.get(0),
    )?;
    Ok(Some(CategorySummary { category, count }))
}

/// The categories a contact is in, with why, in dashboard order.
pub fn contact_categories(
    connection: &Connection,
    contact_id: &str,
) -> Result<Vec<CategoryMembership>> {
    let mut statement = connection.prepare(
        "SELECT cat.*, cm.membership AS membership
           FROM category_members cm JOIN categories cat ON cat.id = cm.category_id
          WHERE cm.contact_id = ?1
          ORDER BY cat.sort_order, cat.name",
    )?;
    let rows = statement.query_map(params![contact_id], |row| {
        Ok(CategoryMembership {
            category: category_from_row(row)?,
            membership: row.get("membership")?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn category_members(
    connection: &Connection,
    category_id: &str,
    query: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<CategoryMembersPage> {
    load(connection, category_id)?;
    let needle = query.map(normalize_text).filter(|value| !value.is_empty());
    let filter = match needle {
        Some(_) => "AND instr(c.normalized_name, ?2) > 0",
        None => "AND ?2 IS NULL",
    };

    let total: i64 = connection.query_row(
        &format!(
            "SELECT COUNT(*) FROM category_members cm JOIN contacts c ON c.id = cm.contact_id
              WHERE cm.category_id = ?1 {filter}"
        ),
        params![category_id, needle],
        |row| row.get(0),
    )?;

    let mut statement = connection.prepare(&format!(
        "SELECT c.*, cm.membership AS membership
           FROM category_members cm JOIN contacts c ON c.id = cm.contact_id
          WHERE cm.category_id = ?1 {filter}
          ORDER BY c.normalized_name, c.id LIMIT ?3 OFFSET ?4"
    ))?;
    let rows = statement.query_map(params![category_id, needle, limit, offset], |row| {
        Ok((contact_from_row(row)?, row.get::<_, String>("membership")?))
    })?;
    let mut items = Vec::new();
    for row in rows {
        let (contact, membership) = row?;
        items.push(CategoryMember { contact: summarize(connection, contact)?, membership });
    }
    Ok(CategoryMembersPage { items, total })
}

/// What a rule would select right now. Reads only; nothing is cached.
pub fn preview_rule(
    connection: &Connection,
    rule: &CategoryRule,
    meaning: Meaning<'_>,
) -> Result<CategoryPreview> {
    let Some(compiled) = compile(connection, rule, meaning)? else {
        return Ok(CategoryPreview { count: 0, sample: Vec::new() });
    };
    let count: i64 = connection.query_row(
        &format!("SELECT COUNT(*) FROM contacts c WHERE c.deleted_at IS NULL AND {}", compiled.sql),
        params_from_iter(compiled.params.iter()),
        |row| row.get(0),
    )?;
    let mut statement = connection.prepare(&format!(
        "SELECT c.* FROM contacts c WHERE c.deleted_at IS NULL AND {}
          ORDER BY c.normalized_name, c.id LIMIT 5",
        compiled.sql
    ))?;
    let rows = statement.query_map(params_from_iter(compiled.params.iter()), contact_from_row)?;
    let mut sample = Vec::new();
    for row in rows {
        sample.push(summarize(connection, row?)?);
    }
    Ok(CategoryPreview { count, sample })
}

/// Shelves the archive suggests: a profession, tag or city that recurs and
/// has no category yet. A few from each source, strongest first.
pub fn suggest_categories(connection: &Connection) -> Result<Vec<CategorySuggestion>> {
    let mut taken: HashSet<String> = HashSet::new();
    {
        let mut statement = connection
            .prepare("SELECT normalized, rule FROM categories WHERE deleted_at IS NULL")?;
        let rows = statement.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
        })?;
        for row in rows {
            let (normalized, raw) = row?;
            taken.insert(normalized);
            if let Some(rule) = parse_rule(raw) {
                for condition in rule.conditions {
                    for value in condition.values {
                        taken.insert(normalize_text(&value));
                    }
                }
            }
        }
    }

    let collect = |sql: &str,
                   field: RuleField,
                   icon: &str,
                   title: &dyn Fn(&str) -> String|
     -> Result<Vec<CategorySuggestion>> {
        let mut statement = connection.prepare(sql)?;
        let rows =
            statement.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)))?;
        let mut found = Vec::new();
        for row in rows {
            let (label, count) = row?;
            let name = title(&label);
            if taken.contains(&normalize_text(&label)) || taken.contains(&normalize_text(&name)) {
                continue;
            }
            found.push(CategorySuggestion {
                name,
                description: None,
                icon: Some(icon.to_string()),
                rule: CategoryRule {
                    combine: RuleMatch::All,
                    conditions: vec![RuleCondition {
                        field,
                        op: RuleOperator::Is,
                        values: vec![label],
                    }],
                },
                count,
            });
            if found.len() == 4 {
                break;
            }
        }
        Ok(found)
    };

    let mut result = collect(
        "SELECT profession, COUNT(*) AS n FROM contacts
          WHERE deleted_at IS NULL AND profession IS NOT NULL AND profession <> ''
          GROUP BY normalized_profession HAVING n >= 3 ORDER BY n DESC, profession LIMIT 40",
        RuleField::Occupation,
        "briefcase",
        &|label| label.to_string(),
    )?;
    result.extend(collect(
        "SELECT t.name, COUNT(DISTINCT ct.contact_id) AS n
           FROM contact_tags ct JOIN tags t ON t.id = ct.tag_id
           JOIN contacts c ON c.id = ct.contact_id AND c.deleted_at IS NULL
          WHERE ct.deleted_at IS NULL AND t.deleted_at IS NULL
          GROUP BY t.id HAVING n >= 3 ORDER BY n DESC, t.name LIMIT 40",
        RuleField::Tag,
        "tag",
        &|label| label.to_string(),
    )?);
    result.extend(collect(
        "SELECT city, COUNT(*) AS n FROM contacts
          WHERE deleted_at IS NULL AND city IS NOT NULL AND city <> ''
          GROUP BY normalized_city HAVING n >= 3 ORDER BY n DESC, city LIMIT 40",
        RuleField::City,
        "map-pin",
        &|label| format!("אנשי קשר ב{label}"),
    )?);
    Ok(result)
}

// ---------------------------------------------------------------------------
// Writing
// ---------------------------------------------------------------------------

fn validate(input: &CategoryInput) -> Result<String> {
    let normalized = normalize_text(&input.name);
    if normalized.is_empty() {
        return Err(DbError::Validation("יש להזין שם קטגוריה".into()));
    }
    if let Some(rule) = &input.rule {
        if rule.conditions.is_empty() {
            return Err(DbError::Validation("כלל צריך לפחות תנאי אחד".into()));
        }
    }
    Ok(normalized)
}

fn rule_json(rule: &Option<CategoryRule>) -> Result<Option<String>> {
    Ok(match rule {
        Some(rule) => Some(serde_json::to_string(rule)?),
        None => None,
    })
}

pub fn create_category(
    connection: &mut Connection,
    input: &CategoryInput,
    meaning: Meaning<'_>,
) -> Result<Category> {
    let normalized = validate(input)?;
    let id = new_id();
    let now = now_iso();
    let sort_order: i64 = connection.query_row(
        "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM categories WHERE deleted_at IS NULL",
        [],
        |row| row.get(0),
    )?;
    let rule = rule_json(&input.rule)?;

    let tx = connection.transaction()?;
    tx.execute(
        "INSERT INTO categories (id, name, normalized, description, parent_id, icon, color, rule,
                                 sort_order, show_on_home, created_at, updated_at, version)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11, 1)",
        params![
            id,
            input.name.trim(),
            normalized,
            input.description,
            input.parent_id,
            input.icon,
            input.color,
            rule,
            sort_order,
            i64::from(input.show_on_home),
            now
        ],
    )?;
    journal(
        &tx,
        "category",
        &id,
        Operation::Create,
        Some(
            &json!({ "name": input.name.trim(), "description": input.description, "rule": input.rule }),
        ),
        None,
        0,
    )?;
    tx.commit()?;

    refresh_category(connection, &id, meaning)?;
    load(connection, &id)
}

pub fn update_category(
    connection: &mut Connection,
    id: &str,
    input: &CategoryInput,
    meaning: Meaning<'_>,
) -> Result<Category> {
    let normalized = validate(input)?;
    let before = load(connection, id)?;
    let version: i64 = connection.query_row(
        "SELECT version FROM categories WHERE id = ?1",
        params![id],
        |row| row.get(0),
    )?;
    let now = now_iso();
    let rule = rule_json(&input.rule)?;
    let name_changed = before.name != input.name.trim();
    let rule_changed = before.rule != input.rule;

    let tx = connection.transaction()?;
    tx.execute(
        "UPDATE categories
            SET name = ?2, normalized = ?3, description = ?4, parent_id = ?5, icon = ?6,
                color = ?7, rule = ?8, show_on_home = ?9, updated_at = ?10, version = ?11
          WHERE id = ?1",
        params![
            id,
            input.name.trim(),
            normalized,
            input.description,
            input.parent_id,
            input.icon,
            input.color,
            rule,
            i64::from(input.show_on_home),
            now,
            version + 1
        ],
    )?;
    let mut changed = serde_json::Map::new();
    let mut was = serde_json::Map::new();
    let mut diff = |key: &str, from: serde_json::Value, to: serde_json::Value| {
        if from != to {
            was.insert(key.to_string(), from);
            changed.insert(key.to_string(), to);
        }
    };
    diff("name", json!(before.name), json!(input.name.trim()));
    diff("description", json!(before.description), json!(input.description));
    diff("icon", json!(before.icon), json!(input.icon));
    diff("color", json!(before.color), json!(input.color));
    diff("rule", json!(before.rule), json!(input.rule));
    diff("showOnHome", json!(before.show_on_home), json!(input.show_on_home));
    journal(
        &tx,
        "category",
        id,
        Operation::Update,
        Some(&serde_json::Value::Object(changed)),
        Some(&serde_json::Value::Object(was)),
        version,
    )?;
    tx.commit()?;

    if rule_changed {
        refresh_category(connection, id, meaning)?;
    }
    if name_changed {
        // The name is indexed text on every member; a rename must reach them.
        let members = member_ids(connection, id)?;
        let tx = connection.transaction()?;
        for contact_id in &members {
            reindex_contact(&tx, contact_id)?;
        }
        tx.commit()?;
    }
    load(connection, id)
}

pub fn delete_category(connection: &mut Connection, id: &str) -> Result<()> {
    let before = load(connection, id)?;
    let affected = member_ids(connection, id)?;
    let now = now_iso();

    let tx = connection.transaction()?;
    tx.execute("UPDATE categories SET deleted_at = ?2 WHERE id = ?1", params![id, now])?;
    tx.execute(
        "UPDATE contact_categories SET deleted_at = ?2 WHERE category_id = ?1 AND deleted_at IS NULL",
        params![id, now],
    )?;
    tx.execute("DELETE FROM category_matches WHERE category_id = ?1", params![id])?;
    journal(
        &tx,
        "category",
        id,
        Operation::Delete,
        None,
        Some(&json!({ "name": before.name })),
        0,
    )?;
    for contact_id in &affected {
        reindex_contact(&tx, contact_id)?;
    }
    tx.commit()?;
    Ok(())
}

/// Persist an order. Ids not listed keep their relative order, after the
/// listed ones.
pub fn reorder_categories(connection: &mut Connection, ids: &[String]) -> Result<()> {
    let listed: HashSet<&String> = ids.iter().collect();
    let rest: Vec<String> = {
        let mut statement = connection.prepare(
            "SELECT id FROM categories WHERE deleted_at IS NULL ORDER BY sort_order, name",
        )?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
            .into_iter()
            .filter(|id| !listed.contains(id))
            .collect()
    };
    let tx = connection.transaction()?;
    for (index, id) in ids.iter().chain(rest.iter()).enumerate() {
        tx.execute(
            "UPDATE categories SET sort_order = ?2 WHERE id = ?1",
            params![id, index as i64],
        )?;
    }
    tx.commit()?;
    Ok(())
}

/// `include` pins a contact in, `exclude` keeps them out, `auto` removes the
/// override. One journal entry per change, keyed to the contact so it shows
/// in the card's history.
pub fn set_membership(
    connection: &mut Connection,
    category_id: &str,
    contact_id: &str,
    mode: &str,
) -> Result<()> {
    if !matches!(mode, "include" | "exclude" | "auto") {
        return Err(DbError::Validation("מצב שיוך אינו מוכר".into()));
    }
    let category = load(connection, category_id)?;
    let exists: Option<i64> = connection
        .query_row(
            "SELECT 1 FROM contacts WHERE id = ?1 AND deleted_at IS NULL",
            params![contact_id],
            |row| row.get(0),
        )
        .optional()?;
    if exists.is_none() {
        return Err(DbError::NotFound("איש הקשר".into()));
    }
    let previous: Option<String> = connection
        .query_row(
            "SELECT mode FROM contact_categories
              WHERE contact_id = ?1 AND category_id = ?2 AND deleted_at IS NULL",
            params![contact_id, category_id],
            |row| row.get(0),
        )
        .optional()?;
    let device = device_id(connection)?;
    let now = now_iso();

    let tx = connection.transaction()?;
    tx.execute(
        "UPDATE contact_categories SET deleted_at = ?3
          WHERE contact_id = ?1 AND category_id = ?2 AND deleted_at IS NULL",
        params![contact_id, category_id, now],
    )?;
    if mode != "auto" {
        tx.execute(
            "INSERT INTO contact_categories (id, contact_id, category_id, mode, created_at, updated_at,
                                             version, device_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5, 1, ?6)",
            params![new_id(), contact_id, category_id, mode, now, device],
        )?;
    }
    let operation = match mode {
        "include" => Operation::Create,
        "exclude" => Operation::Delete,
        _ => Operation::Update,
    };
    journal(
        &tx,
        "contact_category",
        &format!("{category_id}:{contact_id}"),
        operation,
        Some(&json!({
            "contactId": contact_id,
            "categoryId": category_id,
            "categoryName": category.name,
            "mode": mode,
        })),
        Some(&json!({ "mode": previous })),
        0,
    )?;
    reindex_contact(&tx, contact_id)?;
    tx.commit()?;
    Ok(())
}

/// Seed the default shelves into a database that has none — once. A user who
/// later deletes them all is not second-guessed: the marker stays.
pub fn install_defaults(connection: &mut Connection, meaning: Meaning<'_>) -> Result<usize> {
    let done: Option<String> = connection
        .query_row("SELECT value FROM app_meta WHERE key = ?1", params![DEFAULTS_KEY], |row| {
            row.get(0)
        })
        .optional()?;
    if done.is_some() {
        return Ok(0);
    }
    let existing: i64 = connection.query_row(
        "SELECT COUNT(*) FROM categories WHERE deleted_at IS NULL",
        [],
        |row| row.get(0),
    )?;
    let mut installed = 0;
    if existing == 0 {
        let defaults: Vec<CategoryInput> = serde_json::from_str(DEFAULTS_JSON)?;
        for input in &defaults {
            create_category(connection, input, meaning)?;
            installed += 1;
        }
    }
    connection.execute(
        "INSERT OR REPLACE INTO app_meta (key, value, updated_at) VALUES (?1, '1', ?2)",
        params![DEFAULTS_KEY, now_iso()],
    )?;
    Ok(installed)
}

/// The bundled defaults, for tests and for the settings screen.
pub fn default_categories() -> Result<Vec<CategoryInput>> {
    Ok(serde_json::from_str(DEFAULTS_JSON)?)
}
