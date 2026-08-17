import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { IPC_COMMANDS } from './tauri-repository';

/**
 * The IPC boundary is the one place where a rename cannot be caught by the
 * compiler: TypeScript names commands as strings, Rust registers them as
 * functions, and nothing links the two. A mismatch produces a runtime error
 * inside a Tauri window — which is exactly where it is hardest to notice,
 * because that build cannot run on the Linux CI machine.
 *
 * Scanning the Rust source keeps the two halves honest for the price of a
 * regex, and runs everywhere.
 */

const commandsRs = fileURLToPath(new URL('../../src-tauri/src/commands.rs', import.meta.url));
const libRs = fileURLToPath(new URL('../../src-tauri/src/lib.rs', import.meta.url));

function rustCommandNames(): string[] {
  const source = readFileSync(commandsRs, 'utf8');
  return [...source.matchAll(/#\[tauri::command\]\s*pub (?:async )?fn (\w+)/g)].map(
    (match) => match[1]!,
  );
}

function registeredHandlers(): string[] {
  const source = readFileSync(libRs, 'utf8');
  const block = /generate_handler!\[([\s\S]*?)\]/.exec(source);
  if (!block) return [];
  return [...block[1]!.matchAll(/commands::(\w+)/g)].map((match) => match[1]!);
}

describe('IPC command parity', () => {
  it('exposes exactly the commands the Rust shell defines', () => {
    expect([...IPC_COMMANDS].sort()).toEqual(rustCommandNames().sort());
  });

  it('registers every defined command with Tauri', () => {
    // A command can exist in commands.rs and still be unreachable if it was
    // never added to generate_handler!.
    expect(registeredHandlers().sort()).toEqual(rustCommandNames().sort());
  });

  it('has no duplicate command names', () => {
    expect(new Set(IPC_COMMANDS).size).toBe(IPC_COMMANDS.length);
  });
});
