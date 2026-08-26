import { describe, expect, it } from 'vitest';
import { decodeCsvBytes, detectSeparator, parseCsv } from './csv.js';

describe('parseCsv', () => {
  it('parses a plain file', () => {
    const { headers, rows } = parseCsv('שם,טלפון\nאברהם כהן,054-5550134\n');
    expect(headers).toEqual(['שם', 'טלפון']);
    expect(rows).toEqual([['אברהם כהן', '054-5550134']]);
  });

  it('handles quoted fields containing separators, quotes and newlines', () => {
    const text = 'name,notes\n"כהן, אברהם","אמר: ""נדבר אחרי החג""\nלחזור אליו"\n';
    const { rows } = parseCsv(text);
    expect(rows).toEqual([['כהן, אברהם', 'אמר: "נדבר אחרי החג"\nלחזור אליו']]);
  });

  it('strips a BOM and survives CRLF and bare CR line endings', () => {
    const { headers, rows } = parseCsv('﻿a,b\r\n1,2\r3,4\n');
    expect(headers).toEqual(['a', 'b']);
    expect(rows).toEqual([
      ['1', '2'],
      ['3', '4'],
    ]);
  });

  it('drops blank rows and pads short ones to the header width', () => {
    const { rows } = parseCsv('a,b,c\n1,2,3\n,,\n\n4,5\n');
    expect(rows).toEqual([
      ['1', '2', '3'],
      ['4', '5', ''],
    ]);
  });

  it('truncates rows wider than the header', () => {
    const { rows } = parseCsv('a,b\n1,2,3,4\n');
    expect(rows).toEqual([['1', '2']]);
  });

  it('parses semicolon-separated files when told to', () => {
    const { rows } = parseCsv('a;b\n1;2\n', ';');
    expect(rows).toEqual([['1', '2']]);
  });

  it('keeps a final row that has no trailing newline', () => {
    const { rows } = parseCsv('a,b\n1,2');
    expect(rows).toEqual([['1', '2']]);
  });
});

describe('detectSeparator', () => {
  it('prefers the most frequent candidate in the header line', () => {
    expect(detectSeparator('שם,טלפון,עיר\n')).toBe(',');
    expect(detectSeparator('שם;טלפון;עיר\n')).toBe(';');
    expect(detectSeparator('שם\tטלפון\tעיר\n')).toBe('\t');
  });

  it('ignores separators inside quoted headers', () => {
    expect(detectSeparator('"a;b";c;d\ne;f;g\n')).toBe(';');
  });
});

describe('decodeCsvBytes', () => {
  it('decodes clean UTF-8 as UTF-8', () => {
    const bytes = new TextEncoder().encode('שם,עיר\nאברהם,ירושלים\n');
    expect(decodeCsvBytes(bytes)).toContain('ירושלים');
  });

  it('falls back to windows-1255 when UTF-8 decoding produces damage', () => {
    // "שם" in windows-1255: ש=0xF9, ם=0xED — invalid as UTF-8.
    const bytes = new Uint8Array([0xf9, 0xed, 0x2c, 0x31, 0x0a]);
    expect(decodeCsvBytes(bytes)).toBe('שם,1\n');
  });
});
