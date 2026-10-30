/**
 * The rule for the password of a written PDF. It copies `OutputPassword::new` in
 * `crates/core/src/pdf_encrypt.rs` (design §4.3.1, §10.1); change them together. Rust checks the
 * rule again when the password arrives, so a mismatch is refused there as `InvalidParams`.
 * `OUTPUT_PASSWORD_MAX_LENGTH` is checked against Rust by `src/ipc/rustConstants.test.ts`.
 */

/** Longest password in characters. */
export const OUTPUT_PASSWORD_MAX_LENGTH = 127;

/** The printable ASCII characters, from the space to the tilde. */
const FIRST_ALLOWED_CODE = 0x20;
const LAST_ALLOWED_CODE = 0x7e;

/** Whether every character is U+0020–U+007E. An empty string has none outside it. */
export function hasOnlyAllowedCharacters(password: string): boolean {
  for (const character of password) {
    const code = character.codePointAt(0);
    if (
      code === undefined ||
      code < FIRST_ALLOWED_CODE ||
      code > LAST_ALLOWED_CODE
    ) {
      return false;
    }
  }
  return true;
}

/** Whether the password has more characters than {@link OUTPUT_PASSWORD_MAX_LENGTH}. */
export function isOutputPasswordTooLong(password: string): boolean {
  return [...password].length > OUTPUT_PASSWORD_MAX_LENGTH;
}
