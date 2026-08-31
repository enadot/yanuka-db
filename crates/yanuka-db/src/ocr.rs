//! Notebook import: scanned pages, word segmentation, and a correction memory
//! that learns this writer's hand. See ADR-037.
//!
//! There is no production-grade open model for handwritten Hebrew, and the
//! notebooks may never leave the machine — so recognition here is not a
//! pretrained model but a *memory of this writer*: every word the user
//! transcribes is stored as (shape descriptor → text), and each new word box
//! is matched against that memory by cosine similarity. One writer shapes the
//! same word consistently, and a contact notebook's vocabulary repeats — names,
//! towns, professions — so the first pages are typed and the later pages are
//! increasingly pre-filled. Corrections accumulate into a labeled dataset that
//! can later fine-tune a real handwriting model, on this machine.
//!
//! Segmentation is deliberately classical (projection profiles over a
//! binarized image): it is transparent, fast, dependency-light, and its
//! failure mode — a merged or split box — is visible and correctable in the
//! workbench rather than silently wrong.

use base64::Engine as _;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use crate::error::{DbError, Result};

/// Descriptor geometry: a word crop is normalized to this grid. Small enough
/// to keep thousands of corrections cheap to scan, detailed enough that two
/// different Hebrew words rarely collide at the same writer's hand.
const DESC_W: u32 = 48;
const DESC_H: u32 = 16;
const DESC_LEN: usize = (DESC_W * DESC_H) as usize;

/// Cosine similarity at or above this auto-fills a token from memory.
/// Deliberately strict: a wrong auto-fill costs trust, an empty box costs a
/// few keystrokes. Tuned against real pages as they arrive.
const ACCEPT: f32 = 0.92;

/// Pages larger than this are downscaled at import. Keeps the database and
/// the workbench responsive; handwriting stays comfortably legible.
const MAX_DIM: u32 = 2200;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageSummary {
    pub id: String,
    pub file_name: String,
    pub status: String,
    pub contact_id: Option<String>,
    pub width: i64,
    pub height: i64,
    pub tokens: i64,
    pub filled: i64,
    pub imported_at: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Token {
    pub id: String,
    pub line_index: i64,
    pub token_index: i64,
    pub x: i64,
    pub y: i64,
    pub w: i64,
    pub h: i64,
    pub text: Option<String>,
    pub source: String,
    pub confidence: Option<f64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageDetail {
    pub id: String,
    pub file_name: String,
    pub status: String,
    pub contact_id: Option<String>,
    pub width: i64,
    pub height: i64,
    pub image_data_url: String,
    pub tokens: Vec<Token>,
}

/// A word box found by segmentation, before it is stored.
struct FoundBox {
    line_index: i64,
    token_index: i64,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    descriptor: Vec<f32>,
}

// ---------------------------------------------------------------------------
// Import: decode, segment, store.
// ---------------------------------------------------------------------------

pub fn import_page(connection: &Connection, bytes: &[u8], file_name: &str) -> Result<String> {
    let decoded = image::load_from_memory(bytes)
        .map_err(|error| DbError::Validation(format!("קובץ התמונה אינו נתמך: {error}")))?;

    let (w, h) = (decoded.width(), decoded.height());
    let scale_needed = w.max(h) > MAX_DIM;
    let working = if scale_needed {
        let factor = MAX_DIM as f32 / w.max(h) as f32;
        decoded.resize(
            (w as f32 * factor) as u32,
            (h as f32 * factor) as u32,
            image::imageops::FilterType::Triangle,
        )
    } else {
        decoded
    };
    let gray = working.to_luma8();

    // Store the working image as JPEG: scans arrive as photos, and the BLOB
    // must not balloon the encrypted database more than the page is worth.
    let mut stored = Vec::new();
    working
        .write_to(&mut std::io::Cursor::new(&mut stored), image::ImageFormat::Jpeg)
        .map_err(|error| DbError::Validation(format!("שמירת התמונה נכשלה: {error}")))?;

    let boxes = segment(&gray);

    let page_id = crate::new_id();
    let now = crate::now_iso();
    connection.execute(
        "INSERT INTO ocr_pages (id, file_name, image, width, height, status, contact_id, imported_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, 'new', NULL, ?6, ?6)",
        params![page_id, file_name, stored, gray.width() as i64, gray.height() as i64, now],
    )?;

    for found in &boxes {
        let descriptor: Vec<u8> =
            found.descriptor.iter().flat_map(|value| value.to_le_bytes()).collect();
        connection.execute(
            "INSERT INTO ocr_tokens (id, page_id, line_index, token_index, x, y, w, h,
                                     descriptor, text, source, confidence, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL, 'none', NULL, ?10)",
            params![
                crate::new_id(),
                page_id,
                found.line_index,
                found.token_index,
                found.x as i64,
                found.y as i64,
                found.w as i64,
                found.h as i64,
                descriptor,
                now,
            ],
        )?;
    }

    // What the memory already knows fills in immediately: on a well-worn
    // archive a freshly imported page arrives partly transcribed.
    apply_learning(connection, &page_id)?;
    Ok(page_id)
}

// ---------------------------------------------------------------------------
// Segmentation: binarize → line bands → word boxes, right to left.
// ---------------------------------------------------------------------------

fn otsu_threshold(gray: &image::GrayImage) -> u8 {
    let mut histogram = [0u32; 256];
    for pixel in gray.pixels() {
        histogram[pixel.0[0] as usize] += 1;
    }
    let total: u64 = histogram.iter().map(|&count| count as u64).sum();
    let sum_all: u64 = histogram.iter().enumerate().map(|(v, &c)| v as u64 * c as u64).sum();

    let (mut sum_back, mut weight_back) = (0u64, 0u64);
    let (mut best_threshold, mut best_variance) = (127u8, 0.0f64);
    for (value, &count) in histogram.iter().enumerate() {
        weight_back += count as u64;
        if weight_back == 0 {
            continue;
        }
        let weight_fore = total - weight_back;
        if weight_fore == 0 {
            break;
        }
        sum_back += value as u64 * count as u64;
        let mean_back = sum_back as f64 / weight_back as f64;
        let mean_fore = (sum_all - sum_back) as f64 / weight_fore as f64;
        let variance = weight_back as f64 * weight_fore as f64 * (mean_back - mean_fore).powi(2);
        if variance > best_variance {
            best_variance = variance;
            best_threshold = value as u8;
        }
    }
    best_threshold
}

/// Ink mask: true where the pixel is darker than the page. `<=` and not `<`:
/// Otsu places the threshold *on* the dark class's edge, and on a clean scan
/// whose ink is exactly 0 the threshold lands at 0 — a strict comparison
/// would erase every stroke.
fn ink_mask(gray: &image::GrayImage) -> (Vec<bool>, u32, u32) {
    let threshold = otsu_threshold(gray);
    let (w, h) = gray.dimensions();
    let mut mask = vec![false; (w * h) as usize];
    for (x, y, pixel) in gray.enumerate_pixels() {
        if pixel.0[0] <= threshold {
            mask[(y * w + x) as usize] = true;
        }
    }
    (mask, w, h)
}

fn segment(gray: &image::GrayImage) -> Vec<FoundBox> {
    let (mask, width, height) = ink_mask(gray);

    // Line bands from the horizontal ink profile. The floor filters pepper
    // noise and ruled lines that survive binarization only faintly.
    let mut row_ink = vec![0u32; height as usize];
    for y in 0..height {
        for x in 0..width {
            if mask[(y * width + x) as usize] {
                row_ink[y as usize] += 1;
            }
        }
    }
    let floor = (width / 400).max(1);
    let min_line_height = (height / 120).max(6) as usize;

    let mut bands: Vec<(usize, usize)> = Vec::new();
    let mut start: Option<usize> = None;
    for (y, &ink) in row_ink.iter().enumerate() {
        let inked = ink > floor;
        match (inked, start) {
            (true, None) => start = Some(y),
            (false, Some(from)) => {
                if y - from >= min_line_height {
                    bands.push((from, y));
                }
                start = None;
            }
            _ => {}
        }
    }
    if let Some(from) = start {
        if row_ink.len() - from >= min_line_height {
            bands.push((from, row_ink.len()));
        }
    }

    // Merge bands separated by a sliver — descenders split lines otherwise.
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for band in bands {
        match merged.last_mut() {
            Some(last) if band.0 - last.1 < min_line_height / 2 => last.1 = band.1,
            _ => merged.push(band),
        }
    }

    let mut boxes = Vec::new();
    for (line_index, &(top, bottom)) in merged.iter().enumerate() {
        let band_height = bottom - top;

        // Word gaps from the vertical profile inside the band. The gap
        // threshold scales with line height: intra-word letter gaps are a
        // fraction of it, inter-word gaps a multiple.
        let mut col_ink = vec![0u32; width as usize];
        for x in 0..width {
            for y in top..bottom {
                if mask[(y as u32 * width + x) as usize] {
                    col_ink[x as usize] += 1;
                }
            }
        }
        let word_gap = (band_height / 2).max(4);

        let mut spans: Vec<(usize, usize)> = Vec::new();
        let mut span_start: Option<usize> = None;
        let mut gap_run = 0usize;
        for (x, &ink) in col_ink.iter().enumerate() {
            if ink > 0 {
                if span_start.is_none() {
                    span_start = Some(x);
                }
                gap_run = 0;
            } else if let Some(from) = span_start {
                gap_run += 1;
                if gap_run >= word_gap {
                    spans.push((from, x - gap_run + 1));
                    span_start = None;
                    gap_run = 0;
                }
            }
        }
        if let Some(from) = span_start {
            spans.push((from, col_ink.len()));
        }

        // Right to left: the first token the reader meets is the rightmost.
        spans.sort_by_key(|&(left, _)| std::cmp::Reverse(left));

        for (token_index, &(left, right)) in spans.iter().enumerate() {
            let w = (right - left) as u32;
            // Specks and stray marks are not words.
            if w < (band_height / 3).max(3) as u32 {
                continue;
            }
            let (x, y) = (left as u32, top as u32);
            let h = band_height as u32;
            let Some(descriptor) = descriptor_for(gray, &mask, width, x, y, w, h) else {
                continue;
            };
            boxes.push(FoundBox {
                line_index: line_index as i64,
                token_index: token_index as i64,
                x,
                y,
                w,
                h,
                descriptor,
            });
        }
    }
    boxes
}

/// Normalize a word crop into the descriptor grid: tight-trim the ink, sample
/// ink density into DESC_W×DESC_H cells, then L2-normalize so matching is a
/// dot product. Density (not just presence) preserves stroke weight, which is
/// part of a hand's signature.
///
/// The y-scan is padded beyond the line band: a band's height is set by the
/// whole line's ink profile, so the same word on two lines can be clipped at
/// different heights — which would make identical shapes read as different.
/// The pad recovers the word's own ascenders and descenders; it is kept small
/// so a neighboring line's ink stays out.
fn descriptor_for(
    _gray: &image::GrayImage,
    mask: &[bool],
    width: u32,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
) -> Option<Vec<f32>> {
    let height = mask.len() as u32 / width;
    let pad = (h / 3).max(2);
    let y_from = y.saturating_sub(pad);
    let y_to = (y + h + pad).min(height);

    // Tight bounds of actual ink inside the padded box.
    let (mut min_x, mut max_x, mut min_y, mut max_y) = (u32::MAX, 0u32, u32::MAX, 0u32);
    for yy in y_from..y_to {
        for xx in x..x + w {
            if mask[(yy * width + xx) as usize] {
                min_x = min_x.min(xx);
                max_x = max_x.max(xx);
                min_y = min_y.min(yy);
                max_y = max_y.max(yy);
            }
        }
    }
    if min_x == u32::MAX {
        return None;
    }
    let (bw, bh) = (max_x - min_x + 1, max_y - min_y + 1);

    let mut cells = vec![0f32; DESC_LEN];
    for yy in min_y..=max_y {
        for xx in min_x..=max_x {
            if mask[(yy * width + xx) as usize] {
                let cx = ((xx - min_x) as u64 * DESC_W as u64 / bw as u64) as usize;
                let cy = ((yy - min_y) as u64 * DESC_H as u64 / bh as u64) as usize;
                cells[cy.min(DESC_H as usize - 1) * DESC_W as usize
                    + cx.min(DESC_W as usize - 1)] += 1.0;
            }
        }
    }
    let norm = cells.iter().map(|v| v * v).sum::<f32>().sqrt();
    if norm == 0.0 {
        return None;
    }
    for value in &mut cells {
        *value /= norm;
    }
    Some(cells)
}

fn parse_descriptor(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect()
}

// ---------------------------------------------------------------------------
// Learning: corrections in, recognition out.
// ---------------------------------------------------------------------------

/// Record what the user typed for a token, add it to the writer memory, and
/// immediately propagate to matching unfilled tokens on the same page — the
/// visible payoff of correcting a word once.
pub fn record_correction(connection: &Connection, token_id: &str, text: &str) -> Result<()> {
    let row = connection
        .query_row(
            "SELECT page_id, descriptor FROM ocr_tokens WHERE id = ?1",
            params![token_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?)),
        )
        .optional()?;
    let Some((page_id, descriptor)) = row else {
        return Err(DbError::NotFound("המילה".into()));
    };

    let trimmed = text.trim();
    let now = crate::now_iso();
    if trimmed.is_empty() {
        connection.execute(
            "UPDATE ocr_tokens SET text = NULL, source = 'none', confidence = NULL, updated_at = ?2
              WHERE id = ?1",
            params![token_id, now],
        )?;
        return Ok(());
    }

    connection.execute(
        "UPDATE ocr_tokens SET text = ?2, source = 'manual', confidence = NULL, updated_at = ?3
          WHERE id = ?1",
        params![token_id, trimmed, now],
    )?;
    connection.execute(
        "INSERT INTO ocr_corrections (id, descriptor, text, token_id, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![crate::new_id(), descriptor, trimmed, token_id, now],
    )?;
    connection.execute(
        "UPDATE ocr_pages SET status = 'in_progress', updated_at = ?2
          WHERE id = ?1 AND status = 'new'",
        params![page_id, now],
    )?;

    apply_learning(connection, &page_id)?;
    Ok(())
}

/// Match every unfilled token on the page against the correction memory and
/// fill the confident ones. Manual text is never overwritten; a learned fill
/// is refreshed when the memory has gained a better answer.
pub fn apply_learning(connection: &Connection, page_id: &str) -> Result<usize> {
    let memory: Vec<(Vec<f32>, String)> = {
        let mut statement = connection.prepare("SELECT descriptor, text FROM ocr_corrections")?;
        let rows = statement.query_map([], |row| {
            Ok((parse_descriptor(&row.get::<_, Vec<u8>>(0)?), row.get::<_, String>(1)?))
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    if memory.is_empty() {
        return Ok(0);
    }

    let tokens: Vec<(String, Vec<f32>)> = {
        let mut statement = connection.prepare(
            "SELECT id, descriptor FROM ocr_tokens
              WHERE page_id = ?1 AND source != 'manual'",
        )?;
        let rows = statement.query_map(params![page_id], |row| {
            Ok((row.get::<_, String>(0)?, parse_descriptor(&row.get::<_, Vec<u8>>(1)?)))
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };

    let now = crate::now_iso();
    let mut filled = 0usize;
    for (token_id, descriptor) in tokens {
        let mut best: Option<(f32, &str)> = None;
        for (known, text) in &memory {
            let score: f32 = known.iter().zip(&descriptor).map(|(a, b)| a * b).sum();
            if best.map(|(current, _)| score > current).unwrap_or(true) {
                best = Some((score, text));
            }
        }
        if let Some((score, text)) = best {
            if score >= ACCEPT {
                connection.execute(
                    "UPDATE ocr_tokens SET text = ?2, source = 'learned', confidence = ?3,
                                           updated_at = ?4
                      WHERE id = ?1",
                    params![token_id, text, score as f64, now],
                )?;
                filled += 1;
            }
        }
    }
    Ok(filled)
}

/// Autocomplete for the workbench: the archive itself is the writer's
/// vocabulary. Names, professions, places and every previously corrected word
/// are what a notebook page is made of.
pub fn lexicon(connection: &Connection, prefix: &str, limit: i64) -> Result<Vec<String>> {
    let needle = prefix.trim();
    if needle.is_empty() {
        return Ok(Vec::new());
    }
    let pattern = format!("{}%", needle.replace(['%', '_'], " "));
    let mut statement = connection.prepare(
        "SELECT term FROM (
            SELECT text AS term, 0 AS rank FROM ocr_corrections
            UNION SELECT display_name, 1 FROM contacts WHERE deleted_at IS NULL
            UNION SELECT profession, 2 FROM contacts WHERE profession IS NOT NULL AND deleted_at IS NULL
            UNION SELECT city, 2 FROM contacts WHERE city IS NOT NULL AND deleted_at IS NULL
            UNION SELECT name, 2 FROM organizations WHERE deleted_at IS NULL
            UNION SELECT name, 2 FROM tags WHERE deleted_at IS NULL
         ) WHERE term LIKE ?1 GROUP BY term ORDER BY MIN(rank), term LIMIT ?2",
    )?;
    let rows = statement.query_map(params![pattern, limit], |row| row.get::<_, String>(0))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

// ---------------------------------------------------------------------------
// Pages in and out.
// ---------------------------------------------------------------------------

pub fn list_pages(connection: &Connection) -> Result<Vec<PageSummary>> {
    let mut statement = connection.prepare(
        "SELECT p.id, p.file_name, p.status, p.contact_id, p.width, p.height, p.imported_at,
                (SELECT count(*) FROM ocr_tokens t WHERE t.page_id = p.id),
                (SELECT count(*) FROM ocr_tokens t WHERE t.page_id = p.id AND t.text IS NOT NULL)
           FROM ocr_pages p ORDER BY p.imported_at DESC",
    )?;
    let rows = statement.query_map([], |row| {
        Ok(PageSummary {
            id: row.get(0)?,
            file_name: row.get(1)?,
            status: row.get(2)?,
            contact_id: row.get(3)?,
            width: row.get(4)?,
            height: row.get(5)?,
            imported_at: row.get(6)?,
            tokens: row.get(7)?,
            filled: row.get(8)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn get_page(connection: &Connection, page_id: &str) -> Result<PageDetail> {
    let page = connection
        .query_row(
            "SELECT id, file_name, status, contact_id, width, height, image
               FROM ocr_pages WHERE id = ?1",
            params![page_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, Vec<u8>>(6)?,
                ))
            },
        )
        .optional()?;
    let Some((id, file_name, status, contact_id, width, height, blob)) = page else {
        return Err(DbError::NotFound("הדף".into()));
    };

    let mut statement = connection.prepare(
        "SELECT id, line_index, token_index, x, y, w, h, text, source, confidence
           FROM ocr_tokens WHERE page_id = ?1 ORDER BY line_index, token_index",
    )?;
    let rows = statement.query_map(params![page_id], |row| {
        Ok(Token {
            id: row.get(0)?,
            line_index: row.get(1)?,
            token_index: row.get(2)?,
            x: row.get(3)?,
            y: row.get(4)?,
            w: row.get(5)?,
            h: row.get(6)?,
            text: row.get(7)?,
            source: row.get(8)?,
            confidence: row.get(9)?,
        })
    })?;

    Ok(PageDetail {
        id,
        file_name,
        status,
        contact_id,
        width,
        height,
        image_data_url: format!(
            "data:image/jpeg;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(blob)
        ),
        tokens: rows.collect::<rusqlite::Result<Vec<_>>>()?,
    })
}

pub fn delete_page(connection: &Connection, page_id: &str) -> Result<()> {
    let removed = connection.execute("DELETE FROM ocr_pages WHERE id = ?1", params![page_id])?;
    if removed == 0 {
        return Err(DbError::NotFound("הדף".into()));
    }
    Ok(())
}

/// The transcription as text: tokens joined right-to-left within a line,
/// lines top to bottom. Empty tokens become a placeholder so the reader sees
/// where a word is still missing.
pub fn page_text(connection: &Connection, page_id: &str) -> Result<String> {
    let mut statement = connection.prepare(
        "SELECT line_index, text FROM ocr_tokens
          WHERE page_id = ?1 ORDER BY line_index, token_index",
    )?;
    let rows = statement.query_map(params![page_id], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, Option<String>>(1)?))
    })?;

    let mut lines: Vec<Vec<String>> = Vec::new();
    for row in rows {
        let (line_index, text) = row?;
        while lines.len() <= line_index as usize {
            lines.push(Vec::new());
        }
        lines[line_index as usize].push(text.unwrap_or_else(|| "▯".into()));
    }
    Ok(lines
        .into_iter()
        .filter(|line| !line.is_empty())
        .map(|line| line.join(" "))
        .collect::<Vec<_>>()
        .join("\n"))
}

/// Attach the transcription to a contact as a note and close the page out.
pub fn save_as_note(
    connection: &mut Connection,
    page_id: &str,
    contact_id: &str,
) -> Result<String> {
    let (file_name, text) = {
        let name: String = connection
            .query_row("SELECT file_name FROM ocr_pages WHERE id = ?1", params![page_id], |row| {
                row.get(0)
            })
            .optional()?
            .ok_or_else(|| DbError::NotFound("הדף".into()))?;
        (name, page_text(connection, page_id)?)
    };
    if text.trim().is_empty() {
        return Err(DbError::Validation("אין עדיין טקסט מתומלל בדף".into()));
    }

    let body = format!("מתוך מחברת ({file_name}):\n{text}");
    let note_id = crate::taxonomy::add_note(connection, contact_id, &body, false)?;

    connection.execute(
        "UPDATE ocr_pages SET status = 'done', contact_id = ?2, updated_at = ?3 WHERE id = ?1",
        params![page_id, contact_id, crate::now_iso()],
    )?;
    Ok(note_id)
}
