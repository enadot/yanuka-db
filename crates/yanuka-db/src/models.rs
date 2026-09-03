//! Serde structs mirroring the TypeScript domain types.
//!
//! Field names are camelCase on the wire because that is what the frontend
//! consumes; the SQL columns stay snake_case and the mapping happens here, in
//! one place, rather than in every query.

use serde::{Deserialize, Serialize};

use crate::categories::CategoryRule;

pub type Ulid = String;
pub type IsoDateTime = String;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContactSummary {
    pub id: Ulid,
    pub display_name: String,
    pub prefix: Option<String>,
    pub profession: Option<String>,
    pub role: Option<String>,
    pub city: Option<String>,
    pub country: Option<String>,
    pub primary_phone: Option<String>,
    pub tags: Vec<String>,
    pub is_favorite: bool,
    pub updated_at: IsoDateTime,
}

/// A soft-deleted contact as the trash screen shows it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeletedContactSummary {
    #[serde(flatten)]
    pub summary: ContactSummary,
    pub deleted_at: IsoDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContactPhone {
    pub id: Ulid,
    pub contact_id: Ulid,
    pub kind: String,
    /// Exactly as typed. Never rewritten — the original entry is evidence.
    pub raw: String,
    pub e164: Option<String>,
    pub digits: String,
    pub country_code: Option<String>,
    pub is_primary: bool,
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContactEmail {
    pub id: Ulid,
    pub contact_id: Ulid,
    pub kind: String,
    pub address: String,
    pub normalized: String,
    pub is_primary: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContactAlias {
    pub id: Ulid,
    pub contact_id: Ulid,
    pub kind: String,
    pub value: String,
    pub normalized: String,
    pub language_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tag {
    pub id: Ulid,
    pub name: String,
    pub normalized: String,
    pub color: Option<String>,
    pub description: Option<String>,
}

/// A shelf in the archive (ADR-038): a face, a position, and optionally a
/// rule that fills it. Mirrors `Category` in @yanuka/types.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Category {
    pub id: Ulid,
    pub name: String,
    pub normalized: String,
    pub description: Option<String>,
    pub parent_id: Option<Ulid>,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub rule: Option<CategoryRule>,
    pub sort_order: i64,
    pub show_on_home: bool,
}

/// A category with its live size.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CategorySummary {
    #[serde(flatten)]
    pub category: Category,
    pub count: i64,
}

/// A category on a contact's card: `rule` or `manual`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryMembership {
    #[serde(flatten)]
    pub category: Category,
    pub membership: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryMember {
    pub contact: ContactSummary,
    pub membership: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryMembersPage {
    pub items: Vec<CategoryMember>,
    pub total: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryPreview {
    pub count: i64,
    pub sample: Vec<ContactSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CategorySuggestion {
    pub name: String,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub rule: CategoryRule,
    pub count: i64,
}

/// Fields a user can edit on a category. Mirrors `CategoryInputSchema`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CategoryInput {
    pub name: String,
    pub description: Option<String>,
    pub parent_id: Option<Ulid>,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub rule: Option<CategoryRule>,
    pub show_on_home: bool,
}

impl Default for CategoryInput {
    fn default() -> Self {
        Self {
            name: String::new(),
            description: None,
            parent_id: None,
            icon: None,
            color: None,
            rule: None,
            show_on_home: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Organization {
    pub id: Ulid,
    pub name: String,
    pub normalized: String,
    pub kind: String,
    pub city: Option<String>,
    pub region: Option<String>,
    pub country: Option<String>,
    pub address: Option<String>,
    pub notes: Option<String>,
}

/// The core record. Written to and read from `contacts`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Contact {
    pub id: Ulid,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub display_name: String,
    pub prefix: Option<String>,
    pub title: Option<String>,
    pub country: Option<String>,
    pub region: Option<String>,
    pub city: Option<String>,
    pub address: Option<String>,
    pub postal_code: Option<String>,
    pub profession: Option<String>,
    pub role: Option<String>,
    pub notes: Option<String>,
    pub reason_for_saving: Option<String>,
    pub source: Option<String>,
    pub introduced_by: Option<String>,
    pub introduced_by_contact_id: Option<Ulid>,
    pub is_favorite: bool,
    pub last_viewed_at: Option<IsoDateTime>,
    pub created_at: IsoDateTime,
    pub updated_at: IsoDateTime,
    pub created_by: Option<Ulid>,
    pub updated_by: Option<Ulid>,
    pub version: i64,
    pub device_id: Option<String>,
    pub deleted_at: Option<IsoDateTime>,
}

/// What the user can set. Mirrors `ContactInputSchema`; the Rust side revalidates
/// independently because a webview is not a trust boundary.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ContactInput {
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub display_name: String,
    pub prefix: Option<String>,
    pub title: Option<String>,
    pub country: Option<String>,
    pub region: Option<String>,
    pub city: Option<String>,
    pub address: Option<String>,
    pub postal_code: Option<String>,
    pub profession: Option<String>,
    pub role: Option<String>,
    pub notes: Option<String>,
    pub reason_for_saving: Option<String>,
    pub source: Option<String>,
    pub introduced_by: Option<String>,
    pub introduced_by_contact_id: Option<Ulid>,
    pub is_favorite: bool,
    pub phones: Vec<PhoneInput>,
    pub emails: Vec<EmailInput>,
    pub aliases: Vec<AliasInput>,
    pub specialties: Vec<String>,
    pub languages: Vec<String>,
    pub tag_ids: Vec<Ulid>,
    pub category_ids: Vec<Ulid>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PhoneInput {
    pub id: Option<Ulid>,
    pub kind: Option<String>,
    pub raw: String,
    pub label: Option<String>,
    pub is_primary: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct EmailInput {
    pub id: Option<Ulid>,
    pub kind: Option<String>,
    pub address: String,
    pub is_primary: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AliasInput {
    pub id: Option<Ulid>,
    pub kind: Option<String>,
    pub value: String,
    pub language_code: Option<String>,
}

/// A contact joined with every child collection — what the detail screen shows.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContactWithRelations {
    #[serde(flatten)]
    pub contact: Contact,
    pub phones: Vec<ContactPhone>,
    pub emails: Vec<ContactEmail>,
    pub aliases: Vec<ContactAlias>,
    pub tags: Vec<Tag>,
    /// Effective membership: rule matches and manual pins, minus exclusions.
    pub categories: Vec<CategoryMembership>,
    pub specialties: Vec<String>,
    pub languages: Vec<String>,
    pub organizations: Vec<ContactOrganizationLink>,
    pub relationships: Vec<RelationshipEdge>,
    pub contact_notes: Vec<Note>,
}

/// A membership row joined with the organization it points at.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContactOrganizationLink {
    pub id: Ulid,
    pub contact_id: Ulid,
    pub organization_id: Ulid,
    pub role: Option<String>,
    pub is_primary: bool,
    pub started_at: Option<IsoDateTime>,
    pub ended_at: Option<IsoDateTime>,
    pub created_at: IsoDateTime,
    pub organization: Organization,
}

/// A stored, directed relationship read from one of its endpoints.
///
/// `direction` says which endpoint this contact is; the frontend renders the
/// inverse label (`RELATIONSHIP_INVERSES`) when reading an edge from its far
/// end, which is what lets an edge be stored once.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelationshipEdge {
    pub id: Ulid,
    pub from_contact_id: Ulid,
    pub to_contact_id: Ulid,
    #[serde(rename = "type")]
    pub kind: String,
    pub notes: Option<String>,
    pub created_at: IsoDateTime,
    pub direction: String,
    pub other_contact: ContactSummary,
}

/// A timestamped note, separate from `contacts.notes` (the always-visible remark).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    pub id: Ulid,
    pub contact_id: Ulid,
    pub body: String,
    pub is_sensitive: bool,
    pub author_id: Option<Ulid>,
    pub created_at: IsoDateTime,
    pub updated_at: IsoDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchReason {
    pub source: String,
    pub quality: String,
    pub term: String,
    pub snippet: Option<String>,
    pub score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub contact: ContactSummary,
    pub score: f64,
    pub reasons: Vec<MatchReason>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FacetValue {
    pub value: String,
    pub label: String,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResponse {
    pub results: Vec<SearchResult>,
    pub total: i64,
    pub facets: std::collections::HashMap<String, Vec<FacetValue>>,
    pub took_ms: f64,
    pub normalized_terms: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SearchQuery {
    pub text: String,
    pub filters: std::collections::HashMap<String, Vec<String>>,
    pub sort: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub favorites_only: bool,
    pub include_deleted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Page<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
    pub total: i64,
}
