/**
 * CSV parsing for the import flow.
 *
 * Hand-rolled rather than a dependency: the parser runs on an offline machine
 * against files exported from Google Contacts, Outlook and Excel, and RFC 4180
 * plus the two real-world deviations below is the whole problem. A dependency
 * would bring its own quoting opinions and a supply chain, and save none of
 * the code that actually matters here (encoding detection and field mapping).
 *
 * Handled beyond the RFC:
 * - a UTF-8 BOM, which Excel prepends to every export;
 * - bare CR line endings (classic Mac Excel) alongside CRLF and LF.
 */

export interface ParsedCsv {
  /** First row of the file, whitespace-trimmed. */
  headers: string[];
  /** Every subsequent non-empty row, padded/truncated to the header width. */
  rows: string[][];
}

/**
 * Parse CSV text into headers and rows.
 *
 * Quoted fields may contain separators, quotes (doubled) and newlines. A row
 * consisting solely of empty fields is dropped — trailing blank lines are how
 * most exports end, and an all-empty contact row is never meaningful.
 */
export function parseCsv(text: string, separator = ','): ParsedCsv {
  const input = text.charCodeAt(0) === 0xfeff ? text.slice(1) : text;

  const rows: string[][] = [];
  let row: string[] = [];
  let field = '';
  let inQuotes = false;

  const endField = () => {
    row.push(field);
    field = '';
  };
  const endRow = () => {
    endField();
    if (row.some((value) => value.trim() !== '')) {
      rows.push(row);
    }
    row = [];
  };

  for (let i = 0; i < input.length; i += 1) {
    const char = input[i];

    if (inQuotes) {
      if (char === '"') {
        if (input[i + 1] === '"') {
          field += '"';
          i += 1;
        } else {
          inQuotes = false;
        }
      } else {
        field += char;
      }
      continue;
    }

    if (char === '"' && field === '') {
      inQuotes = true;
    } else if (char === separator) {
      endField();
    } else if (char === '\n') {
      endRow();
    } else if (char === '\r') {
      if (input[i + 1] === '\n') {
        i += 1;
      }
      endRow();
    } else {
      field += char;
    }
  }
  if (field !== '' || row.length > 0) {
    endRow();
  }

  const [headerRow = [], ...dataRows] = rows;
  const headers = headerRow.map((header) => header.trim());
  const width = headers.length;
  const normalized = dataRows.map((cells) => {
    const padded = cells.slice(0, width);
    while (padded.length < width) {
      padded.push('');
    }
    return padded;
  });

  return { headers, rows: normalized };
}

/**
 * Decode a CSV file's bytes to text.
 *
 * UTF-8 first; when that produces replacement characters the bytes are retried
 * as windows-1255, the encoding of Hebrew Excel/Outlook exports old enough to
 * predate Unicode-by-default. Detection by damage rather than by heuristics:
 * a file that decodes cleanly as UTF-8 *is* UTF-8.
 */
export function decodeCsvBytes(bytes: Uint8Array): string {
  const utf8 = new TextDecoder('utf-8').decode(bytes);
  if (!utf8.includes('�')) {
    return utf8;
  }
  try {
    return new TextDecoder('windows-1255').decode(bytes);
  } catch {
    return utf8;
  }
}

/**
 * Guess the separator by counting candidates in the header line.
 *
 * Hebrew locales of Excel export "CSV" with semicolons (the comma is the
 * decimal separator there), and tab-separated exports are pasted often enough
 * to be worth catching.
 */
export function detectSeparator(text: string): string {
  const firstLine = text.slice(0, text.indexOf('\n') === -1 ? undefined : text.indexOf('\n'));
  let best = ',';
  let bestCount = 0;
  for (const candidate of [',', ';', '\t']) {
    let count = 0;
    let quoted = false;
    for (const char of firstLine) {
      if (char === '"') {
        quoted = !quoted;
      } else if (!quoted && char === candidate) {
        count += 1;
      }
    }
    if (count > bestCount) {
      best = candidate;
      bestCount = count;
    }
  }
  return best;
}
