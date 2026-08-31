//! Notebook import end to end: segmentation finds the words, a correction
//! teaches the writer memory, and the same shape is recognized from then on —
//! on the same page and on pages imported later. Runs with `--features ocr`.
#![cfg(feature = "ocr")]

use image::{GrayImage, Luma, RgbImage};
use yanuka_db::models::ContactInput;
use yanuka_db::{migrate, ocr, repository};

/// A deterministic pseudo-word: the same seed inks the same strokes, the way
/// one writer shapes one word the same way. `jitter` adds a stray pixel — no
/// two real pen strokes are identical.
fn draw_word(img: &mut GrayImage, x0: u32, y0: u32, seed: u64, jitter: bool) {
    let mut state = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    let mut next = || {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (state >> 33) as u32
    };
    // A baseline stroke plus a handful of seed-dependent verticals and dots.
    for x in 0..48 {
        img.put_pixel(x0 + x, y0 + 16, Luma([0]));
        img.put_pixel(x0 + x, y0 + 17, Luma([0]));
    }
    for _ in 0..6 {
        let sx = next() % 44;
        let sh = 6 + next() % 10;
        for y in 0..sh {
            img.put_pixel(x0 + sx, y0 + 4 + y, Luma([0]));
            img.put_pixel(x0 + sx + 1, y0 + 4 + y, Luma([0]));
        }
    }
    if jitter {
        img.put_pixel(x0 + 3, y0 + 5, Luma([0]));
    }
}

/// A synthetic page: rows of pseudo-words with generous word gaps.
fn page(words: &[&[u64]], jitter: bool) -> Vec<u8> {
    let mut img = GrayImage::from_pixel(640, 200, Luma([255]));
    for (line, seeds) in words.iter().enumerate() {
        for (index, &seed) in seeds.iter().enumerate() {
            // Rightmost word first, as a Hebrew hand writes.
            let x0 = 640 - 80 - (index as u32) * 120;
            let y0 = 30 + (line as u32) * 60;
            draw_word(&mut img, x0, y0, seed, jitter);
        }
    }
    let rgb: RgbImage = image::DynamicImage::ImageLuma8(img).to_rgb8();
    let mut bytes = Vec::new();
    image::DynamicImage::ImageRgb8(rgb)
        .write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png)
        .unwrap();
    bytes
}

fn open_db(dir: &tempfile::TempDir) -> yanuka_db::rusqlite::Connection {
    let mut connection = yanuka_db::open(&dir.path().join("contacts.db"), None).unwrap();
    migrate(&mut connection).unwrap();
    connection
}

#[test]
fn a_correction_teaches_the_writer_memory_across_pages() {
    let dir = tempfile::tempdir().unwrap();
    let connection = open_db(&dir);

    // Two lines, five words; the word "seed 1" appears on both lines.
    let first =
        ocr::import_page(&connection, &page(&[&[1, 2, 3], &[4, 1]], false), "מחברת-א.png").unwrap();

    let detail = ocr::get_page(&connection, &first).unwrap();
    assert_eq!(detail.tokens.len(), 5, "segmentation must find every word");
    assert_eq!(detail.tokens.iter().filter(|t| t.line_index == 0).count(), 3);
    // RTL: token 0 of a line is the rightmost box.
    let line0: Vec<_> = detail.tokens.iter().filter(|t| t.line_index == 0).collect();
    assert!(line0[0].x > line0[1].x && line0[1].x > line0[2].x);

    // Correct the rightmost word of line 0 (seed 1). The same shape sits on
    // line 1 — the page must fill it in on its own.
    ocr::record_correction(&connection, &line0[0].id, "אברהם").unwrap();
    let detail = ocr::get_page(&connection, &first).unwrap();
    let learned: Vec<_> = detail.tokens.iter().filter(|t| t.source == "learned").collect();
    assert_eq!(learned.len(), 1, "the twin shape on the other line auto-fills");
    assert_eq!(learned[0].text.as_deref(), Some("אברהם"));
    assert!(learned[0].confidence.unwrap() >= 0.92);

    // A page imported later — jittered, as real ink is — arrives pre-filled.
    let second = ocr::import_page(&connection, &page(&[&[5, 1]], true), "מחברת-ב.png").unwrap();
    let detail = ocr::get_page(&connection, &second).unwrap();
    let known: Vec<_> = detail.tokens.iter().filter(|t| t.source == "learned").collect();
    assert_eq!(known.len(), 1, "the memory recognizes the word on a new page");
    assert_eq!(known[0].text.as_deref(), Some("אברהם"));

    // The corrected vocabulary reaches the autocomplete lexicon.
    let terms = ocr::lexicon(&connection, "אבר", 8).unwrap();
    assert!(terms.contains(&"אברהם".to_string()));
}

#[test]
fn a_transcribed_page_becomes_a_searchable_note() {
    let dir = tempfile::tempdir().unwrap();
    let mut connection = open_db(&dir);

    let page_id = ocr::import_page(&connection, &page(&[&[7, 8]], false), "מחברת-ג.png").unwrap();
    let detail = ocr::get_page(&connection, &page_id).unwrap();
    for (token, word) in detail.tokens.iter().zip(["ממליץ", "מלונדון"]) {
        ocr::record_correction(&connection, &token.id, word).unwrap();
    }
    assert_eq!(ocr::page_text(&connection, &page_id).unwrap(), "ממליץ מלונדון");

    let created = repository::create_contact(
        &mut connection,
        &ContactInput { display_name: "אריה גולד".into(), ..Default::default() },
        None,
    )
    .unwrap();
    ocr::save_as_note(&mut connection, &page_id, &created.contact.id).unwrap();

    // The note is a real note: journaled, indexed, findable.
    let found = yanuka_db::search::search(
        &connection,
        &yanuka_db::models::SearchQuery { text: "מלונדון".into(), ..Default::default() },
    )
    .unwrap();
    assert_eq!(found.results.len(), 1);
    assert_eq!(found.results[0].contact.id, created.contact.id);

    let status: String = connection
        .query_row("SELECT status FROM ocr_pages WHERE id = ?1", [&page_id], |row| row.get(0))
        .unwrap();
    assert_eq!(status, "done");

    // Deleting the page removes its tokens with it; the note stays.
    ocr::delete_page(&connection, &page_id).unwrap();
    let tokens: i64 = connection
        .query_row("SELECT count(*) FROM ocr_tokens WHERE page_id = ?1", [&page_id], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(tokens, 0);
}
