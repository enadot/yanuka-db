//! Smart categories (ADR-038) against a real SQLite database: rules select,
//! the cache follows every write, overrides win, and the search index sees
//! the effective membership.

use yanuka_db::categories::{
    self, CategoryRule, RuleCondition, RuleField, RuleMatch, RuleOperator,
};
use yanuka_db::models::{CategoryInput, ContactInput, PhoneInput, SearchQuery};
use yanuka_db::{migrate, open_in_memory, repository, search, taxonomy};

fn db() -> yanuka_db::rusqlite::Connection {
    let mut connection = open_in_memory().expect("open");
    migrate(&mut connection).expect("migrate");
    connection
}

fn person(name: &str, profession: &str, country: Option<&str>) -> ContactInput {
    ContactInput {
        display_name: name.to_string(),
        profession: Some(profession.to_string()),
        country: country.map(str::to_string),
        ..Default::default()
    }
}

fn contains(field: RuleField, values: &[&str]) -> RuleCondition {
    RuleCondition {
        field,
        op: RuleOperator::Contains,
        values: values.iter().map(|v| v.to_string()).collect(),
    }
}

fn rule(combine: RuleMatch, conditions: Vec<RuleCondition>) -> CategoryRule {
    CategoryRule { combine, conditions }
}

fn category(name: &str, rule: Option<CategoryRule>) -> CategoryInput {
    CategoryInput { name: name.to_string(), rule, ..Default::default() }
}

fn member_names(connection: &yanuka_db::rusqlite::Connection, id: &str) -> Vec<String> {
    categories::category_members(connection, id, None, 100, 0)
        .unwrap()
        .items
        .into_iter()
        .map(|member| member.contact.display_name)
        .collect()
}

#[test]
fn a_rule_selects_rabbis_abroad_and_nobody_else() {
    let mut connection = db();
    repository::create_contact(
        &mut connection,
        &person("הרב לונדון", "רב קהילה", Some("GB")),
        None,
    )
    .unwrap();
    repository::create_contact(&mut connection, &person("הרב ירושלים", "רב", Some("IL")), None)
        .unwrap();
    // "ערב" contains the letters of רב but is not a word starting with it.
    repository::create_contact(&mut connection, &person("סוחר", "סוחר ערבים", Some("GB")), None)
        .unwrap();

    let abroad = categories::create_category(
        &mut connection,
        &category(
            "רבנים בחו\"ל",
            Some(rule(
                RuleMatch::All,
                vec![
                    contains(RuleField::Occupation, &["רב", "דיין"]),
                    RuleCondition {
                        field: RuleField::Country,
                        op: RuleOperator::IsNot,
                        values: vec!["IL".into()],
                    },
                ],
            )),
        ),
        None,
    )
    .unwrap();

    assert_eq!(member_names(&connection, &abroad.id), vec!["הרב לונדון"]);
    let summary = categories::get_category(&connection, &abroad.id).unwrap().unwrap();
    assert_eq!(summary.count, 1);
}

#[test]
fn membership_follows_every_write() {
    let mut connection = db();
    let scribes = categories::create_category(
        &mut connection,
        &category(
            "סופרי סת\"ם",
            Some(rule(RuleMatch::All, vec![contains(RuleField::Occupation, &["סת\"ם"])])),
        ),
        None,
    )
    .unwrap();

    let created =
        repository::create_contact(&mut connection, &person("לומד", "מלמד", None), None).unwrap();
    assert!(member_names(&connection, &scribes.id).is_empty());

    // Gershayim are normalized on both sides: סתם matches סת"ם.
    let mut input = person("לומד", "סופר סתם", None);
    input.display_name = "לומד".into();
    repository::update_contact(&mut connection, &created.contact.id, &input, None).unwrap();
    assert_eq!(member_names(&connection, &scribes.id), vec!["לומד"]);

    let card = repository::get_contact(&connection, &created.contact.id).unwrap().unwrap();
    assert_eq!(card.categories.len(), 1);
    assert_eq!(card.categories[0].membership, "rule");

    repository::delete_contact(&mut connection, &created.contact.id).unwrap();
    assert!(member_names(&connection, &scribes.id).is_empty());

    repository::restore_contact(&mut connection, &created.contact.id).unwrap();
    assert_eq!(member_names(&connection, &scribes.id), vec!["לומד"]);
}

#[test]
fn a_note_can_put_someone_on_a_shelf() {
    let mut connection = db();
    let from_notebooks = categories::create_category(
        &mut connection,
        &category(
            "מהמחברות",
            Some(rule(RuleMatch::All, vec![contains(RuleField::Notes, &["מתוך מחברת"])])),
        ),
        None,
    )
    .unwrap();
    let created =
        repository::create_contact(&mut connection, &person("נרשם", "עסקן", None), None).unwrap();
    assert!(member_names(&connection, &from_notebooks.id).is_empty());

    taxonomy::add_note(&mut connection, &created.contact.id, "מתוך מחברת (עמוד 3): שלום", false)
        .unwrap();
    assert_eq!(member_names(&connection, &from_notebooks.id), vec!["נרשם"]);
}

#[test]
fn overrides_win_over_the_rule_and_can_be_lifted() {
    let mut connection = db();
    let scribes = categories::create_category(
        &mut connection,
        &category(
            "סופרים",
            Some(rule(RuleMatch::All, vec![contains(RuleField::Occupation, &["סופר"])])),
        ),
        None,
    )
    .unwrap();
    let scribe =
        repository::create_contact(&mut connection, &person("סופר מודר", "סופר", None), None)
            .unwrap();
    let outsider =
        repository::create_contact(&mut connection, &person("מצורף ביד", "נגר", None), None)
            .unwrap();

    categories::set_membership(&mut connection, &scribes.id, &scribe.contact.id, "exclude")
        .unwrap();
    categories::set_membership(&mut connection, &scribes.id, &outsider.contact.id, "include")
        .unwrap();
    let members = categories::category_members(&connection, &scribes.id, None, 100, 0).unwrap();
    assert_eq!(members.items.len(), 1);
    assert_eq!(members.items[0].contact.display_name, "מצורף ביד");
    assert_eq!(members.items[0].membership, "manual");

    // The override is on the record.
    let history = yanuka_db::mutation::history(&connection, Some(&scribe.contact.id), 20).unwrap();
    assert!(history.iter().any(|entry| entry["entityType"] == "contact_category"));

    categories::set_membership(&mut connection, &scribes.id, &scribe.contact.id, "auto").unwrap();
    categories::set_membership(&mut connection, &scribes.id, &outsider.contact.id, "auto").unwrap();
    assert_eq!(member_names(&connection, &scribes.id), vec!["סופר מודר"]);
}

#[test]
fn the_search_index_sees_rule_membership() {
    let mut connection = db();
    repository::create_contact(&mut connection, &person("שרברב מהכלל", "שרברב", None), None)
        .unwrap();
    categories::create_category(
        &mut connection,
        &category(
            "אינסטלציה",
            Some(rule(RuleMatch::All, vec![contains(RuleField::Occupation, &["שרברב"])])),
        ),
        None,
    )
    .unwrap();

    // By the category's name, as free text…
    let by_name = search::search(
        &connection,
        &SearchQuery { text: "אינסטלציה".into(), ..Default::default() },
    )
    .unwrap();
    assert_eq!(by_name.results.len(), 1);
    assert_eq!(by_name.results[0].contact.display_name, "שרברב מהכלל");

    // …and as a facet filter.
    let mut filters = std::collections::HashMap::new();
    filters.insert("category".to_string(), vec!["אינסטלציה".to_string()]);
    let filtered = search::search(
        &connection,
        &SearchQuery { text: String::new(), filters, ..Default::default() },
    )
    .unwrap();
    assert_eq!(filtered.results.len(), 1);
    assert_eq!(filtered.facets.get("category").map(|values| values[0].count), Some(1));
}

#[test]
fn editing_the_rule_reselects_and_a_preview_writes_nothing() {
    let mut connection = db();
    repository::create_contact(&mut connection, &person("נגר לבדיקה", "נגר", None), None).unwrap();
    repository::create_contact(&mut connection, &person("סופר לבדיקה", "סופר", None), None)
        .unwrap();
    let shelf = categories::create_category(
        &mut connection,
        &category(
            "כלל שמתחלף",
            Some(rule(RuleMatch::All, vec![contains(RuleField::Occupation, &["סופר"])])),
        ),
        None,
    )
    .unwrap();

    let carpenters = rule(RuleMatch::All, vec![contains(RuleField::Occupation, &["נגר"])]);
    let preview = categories::preview_rule(&connection, &carpenters, None).unwrap();
    assert_eq!(preview.count, 1);
    assert_eq!(preview.sample[0].display_name, "נגר לבדיקה");
    assert_eq!(member_names(&connection, &shelf.id), vec!["סופר לבדיקה"]);

    let updated = categories::update_category(
        &mut connection,
        &shelf.id,
        &CategoryInput {
            name: "נגרים".into(),
            rule: Some(carpenters),
            show_on_home: false,
            ..Default::default()
        },
        None,
    )
    .unwrap();
    assert_eq!(updated.name, "נגרים");
    assert!(!updated.show_on_home);
    assert_eq!(member_names(&connection, &shelf.id), vec!["נגר לבדיקה"]);

    // The rename reached the index: the carpenter is found by the new name.
    let by_new_name =
        search::search(&connection, &SearchQuery { text: "נגרים".into(), ..Default::default() })
            .unwrap();
    assert_eq!(by_new_name.results.len(), 1);
}

#[test]
fn empty_checks_and_recency() {
    let mut connection = db();
    let mut with_phone = person("עם טלפון", "עסקן", None);
    with_phone.phones = vec![PhoneInput { raw: "054-555-0101".into(), ..Default::default() }];
    repository::create_contact(&mut connection, &with_phone, None).unwrap();
    repository::create_contact(&mut connection, &person("בלי טלפון", "עסקן", None), None).unwrap();

    let missing = categories::create_category(
        &mut connection,
        &category(
            "חסרי טלפון",
            Some(rule(
                RuleMatch::All,
                vec![RuleCondition {
                    field: RuleField::Phone,
                    op: RuleOperator::IsEmpty,
                    values: vec![],
                }],
            )),
        ),
        None,
    )
    .unwrap();
    assert_eq!(member_names(&connection, &missing.id), vec!["בלי טלפון"]);

    let recent = categories::create_category(
        &mut connection,
        &category(
            "נוספו לאחרונה",
            Some(rule(
                RuleMatch::All,
                vec![RuleCondition {
                    field: RuleField::Created,
                    op: RuleOperator::WithinDays,
                    values: vec!["30".into()],
                }],
            )),
        ),
        None,
    )
    .unwrap();
    assert_eq!(member_names(&connection, &recent.id).len(), 2);
}

#[test]
fn order_delete_and_suggestions() {
    let mut connection = db();
    let first =
        categories::create_category(&mut connection, &category("סדר א", None), None).unwrap();
    let second =
        categories::create_category(&mut connection, &category("סדר ב", None), None).unwrap();
    categories::reorder_categories(&mut connection, &[second.id.clone(), first.id.clone()])
        .unwrap();
    let ids: Vec<String> = categories::list_categories(&connection)
        .unwrap()
        .into_iter()
        .map(|summary| summary.category.id)
        .collect();
    assert_eq!(ids, vec![second.id.clone(), first.id.clone()]);

    categories::delete_category(&mut connection, &first.id).unwrap();
    assert!(categories::get_category(&connection, &first.id).unwrap().is_none());

    for index in 0..4 {
        repository::create_contact(
            &mut connection,
            &person(&format!("שדכן {index}"), "שדכן", None),
            None,
        )
        .unwrap();
    }
    let suggestions = categories::suggest_categories(&connection).unwrap();
    let matchmakers = suggestions.iter().find(|s| s.name == "שדכן").expect("suggested");
    assert_eq!(matchmakers.count, 4);
    assert_eq!(matchmakers.rule.conditions[0].values, vec!["שדכן".to_string()]);
}

#[test]
fn defaults_install_once_and_only_into_an_empty_archive() {
    let mut connection = db();
    let installed = categories::install_defaults(&mut connection, None).unwrap();
    assert_eq!(installed, categories::default_categories().unwrap().len());
    assert_eq!(categories::install_defaults(&mut connection, None).unwrap(), 0);

    // The bundled rules parse, and a rabbi in London lands on the right shelf.
    repository::create_contact(
        &mut connection,
        &person("הרב לונדון", "רב קהילה", Some("GB")),
        None,
    )
    .unwrap();
    let shelves: Vec<String> = repository::get_contact(&connection, &{
        let list = repository::list_contacts(&connection, None, 10, None).unwrap();
        list.items[0].id.clone()
    })
    .unwrap()
    .unwrap()
    .categories
    .into_iter()
    .map(|c| c.category.name)
    .collect();
    assert!(shelves.contains(&"רבנים בחו\"ל".to_string()));
    assert!(shelves.contains(&"נוספו לאחרונה".to_string()));
    assert!(!shelves.contains(&"רבנים בארץ".to_string()));

    // A second, non-empty archive without the marker is left alone.
    let mut other = db();
    categories::create_category(&mut other, &category("שלי", None), None).unwrap();
    assert_eq!(categories::install_defaults(&mut other, None).unwrap(), 0);
    assert_eq!(categories::list_categories(&other).unwrap().len(), 1);
}
