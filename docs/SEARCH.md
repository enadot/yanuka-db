# SEARCH

The search *is* the product. Everything here follows from one job: find a person
when the only thing remembered is a fragment — a profession, a city, who made
the introduction, or a sentence somebody wrote about them years ago.

Implemented in `packages/search` (TypeScript) and `crates/yanuka-search` +
`crates/yanuka-db/src/search.rs` (Rust). The two are held to a shared fixture;
see ARCHITECTURE.md.

## Normalization

Applied to every indexed field and every query, in this order. Order matters —
diacritics come off before punctuation so that a niqqud mark adjacent to a
geresh cannot block the match.

| step | `שָׁלוֹם` → | why |
|---|---|---|
| 1. strip bidi/invisible | | U+200E…U+202E ride along with text copied out of RTL documents |
| 2. NFD, drop combining marks | `שלום` | niqqud, te'amim, and Latin accents (`José` → `Jose`) |
| 3. remove geresh/gershayim | `סת"ם` → `סתם` | U+05F3/U+05F4 and the ASCII and curly forms all behave alike |
| 4. maqaf and dashes → space | `בן־גוריון` → `בן גוריון` | |
| 5. fold final letters | `שלום` → `שלומ` | `ך ם ן ף ץ` are positional only, and users type them inconsistently |
| 6. lowercase | | folds Latin, no-op for Hebrew |
| 7. collapse whitespace | | |

Idempotent, and tested to be — a value read back and reindexed must not change.

## Honorifics

Not prefix-stripping; a separate, curated mechanism. `הרב`, `רבי`, `ר'`,
`הגאון`, `האדמו"ר`, `הרה"ג`, `ד"ר`, `פרופ'`, `Rabbi`, `Dr`… are removed from the
*head* of a name, repeatedly, so `הרב הגאון רבי משה כהן` and `משה כהן` land on
the same key. Applied to stored names and to the query alike.

It never consumes the last token. `הרב` on its own is somebody's whole name as
recorded, and returning an empty string would make the record unsearchable.

## Hebrew proclitics — the part that needs care

`ה ו ב ל מ כ ש` attach to words: `מלונדון`, `בירושלים`, `בתפילין`. Stripping them
blindly is a false-positive generator:

```
מלון → לון      שלום → לום      בית → ית      לוי → וי
משה  → שה       הרב  → רב       בני → ני      כהן → הן
```

Every one of those is asserted in `normalize.test.ts` and in the shared fixture.

The rule: **a proclitic is only stripped when at least four letters remain.**
That keeps `מלונדון → לונדון` and `בירושלים → ירושלים` while leaving every common
short word intact.

Where the expansion happens differs by field, and the asymmetry is deliberate:

- **Query side, all fields.** The user types `מלונדון`; the city is stored as
  `לונדון`. Variants are OR-ed into the MATCH expression and score at
  `PROCLITIC_PENALTY` (×0.5), so a hit on the literal query always outranks a
  hit on a grammatical guess.
- **Index side, free text only.** The note says `בתפילין`; the user types
  `תפילין`. FTS5 tokenizes `בתפילין` as one word, so no query expansion can
  reach it. Notes and `reason_for_saving` therefore index the stripped form as
  well. Names never do — a spurious variant in the highest-weighted field is not
  worth the recall.

## Layers

Each runs only when the previous one has not answered the question.

**1. Phone.** A bare run of digits (≥4, no letters) is a number lookup, not
text. Matched on the last seven digits of `contact_phones.digits`, so
`054-555-0134`, `+972545550134` and `5550134` all reach the same record.

**2. Full text.** FTS5 `MATCH`, ordered by `bm25` with per-column weights. Terms
are AND-ed so that adding a word narrows; variants within a term are OR-ed; the
last term gets `*` so results appear while it is still being typed.

**3. Fuzzy.** Only when the first two returned fewer than twenty hits — always
running it would cost time on every keystroke and dilute good results. Trigram
overlap proposes candidates, Damerau-Levenshtein ranks them.

The fuzzy layer preserves the AND semantics: a contact is proposed only if
*every* term matches it closely, and the weakest term governs the score.
Scoring terms independently and unioning would make a two-word search return
*more* rows than the one-word search it refines — the opposite of what typing
another word means.

**4. Semantic** (desktop only, ADR-036). A local embedding model
(multilingual-e5-small, int8 ONNX, bundled — nothing leaves the machine)
matches the *meaning* of a query against every note and contact profile:
`עסקן מאנגליה שעוזר עם בתי כנסת` finds the note that says
`יהודי מלונדון… יכול לסייע בבניית בתי כנסת` with no word in common. Gated
like fuzzy — a multi-word query, or a lexical search that came back thin —
and additive only: it proposes contacts the lexical layers missed, never
re-ranks one they found, and its score ceiling sits below a direct name hit.
A semantic hit carries the matching note as its snippet, labeled
`לפי משמעות`. When the model is missing the layer silently does not exist.

## Phonetic keys

For names only, and only to retrieve candidates — never as a match on their own.

- **Hebrew**: drop interior matres lectionis (`א ה ו י ע`), fold sound-alike
  letters. `פרידמן` and `פרידמאן` collapse to the same key.
- **Latin**: fold digraphs (`ph→f`, `ck→k`, `tz→z`), drop interior vowels, keep
  a leading vowel. `Friedman` = `Freidman`; `Moshe` = `Moishe`; `Cohen` = `Kohen`.

Stored alongside the real spelling in the `name` column, so a misspelled query
is answered by the same index rather than a second lookup.

## Ranking

```
score = Σ_field  weight[field] × quality[match] × penalty

weight    name 100 · phone 100 · alias 90 · email 60 · profession 50
          tag 40 · category 40 · organization 35 · specialty 35
          city 30 · role 30 · reason_for_saving 25 · country 20 · notes 20

quality   exact 1.00 · prefix 0.80 · fulltext 0.55 · fuzzy 0.45
penalty   ×0.5 when the hit came from a proclitic-stripped variant

bonus     favourite +12 · recently viewed +8 (decaying over a week)
          has phone +3 · has organization +2
```

Two things the formula encodes deliberately:

**Notes score lowest per hit** because they are long and therefore easy to match
by accident. They are never excluded — they are what makes this archive worth
keeping — only ranked below the precise signals.

**Repeated hits in one field do not add up.** Each field contributes its best
match at full value and further matches at 15%. Five occurrences of a word in
one long note must not outrank a single exact name match, and a contact matching
across *several different* fields is the real signal that it is what was meant.

`bm25` orders results *within* the full-text layer only. Its values are
comparable within one query and nowhere else, so they are never weighed against
the exact or fuzzy layers.

## Facets

The `category` facet — and the `category` column of the FTS document — read
the `category_members` view, so a contact selected by a smart category's
rule is filterable and findable by that category's name exactly like one
assigned by hand (ADR-038).

Counted over the matched set in the same pass as the results, so the rows, the
total and the filter counts are one round trip.

Values within a field are OR-ed, different fields are AND-ed.

**Known limitation.** Counts are computed *after* the active filters are
applied, so selecting a country shows the other countries at zero. Proper
per-dimension exclusion needs one pass per facet. Deferred; the UI compensates
by keeping selected chips visible with a clear-all next to them.

## Explaining a result

Every hit carries its `MatchReason`s: which field, how well it matched, and —
for notes — a snippet of the surrounding text.

The snippet needs care. The term reaching that code has been folded, so it no
longer occurs literally in what the user wrote and `indexOf` always misses.
Rather than mapping normalized offsets back through a length-changing
transformation, `snippetAroundNormalized` walks the original word by word,
normalizes each, and cuts around the first match. Offsets stay anchored to the
untouched source, so the user sees exactly what they typed — gershayim included.

A note match is also kept even when a stronger field matched the same term. Its
score is not why it is worth reporting; the sentence is.

## Performance

Target: under 100 ms for a local search at 100,000 contacts, measured as query
time in Rust — excluding React render and IPC, which are a separate budget.

| stage | budget |
|---|---|
| FTS5 MATCH + bm25, top 500 | 15 ms |
| trigram candidates (only when < 20 hits) | 20 ms |
| rerank | 2 ms |
| hydrate top 50 | 4 ms |
| facets | 12 ms |

The single biggest lever is that layer 3 rarely runs. The UI adds a 150 ms
debounce so a search is issued per word, not per keystroke.

## Deferred

- **Algorithmic Hebrew↔Latin transliteration.** `Cohen`/`Kohen`/`Kohn`/`Cahn`/
  `Kagan` is a research problem. What ships instead: both scripts land in the
  same FTS row, aliases are a first-class entity weighted at 90, and the
  phonetic key covers the common families.
- **Matres lectionis folding at the index level.** `דן`/`דין` and `רב`/`ריב` are
  different words. Handled by the fuzzy layer at edit distance 1 instead, with a
  score discount.
- **Natural-language queries** (`מי יש לנו בלונדון שקשור לחינוך`). The semantic
  layer (ADR-036) now answers the free-text half of this; the structured fields
  remain available for a filter-parsing pass later.
