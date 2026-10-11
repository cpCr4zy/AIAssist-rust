# Security policy

## Reporting a vulnerability

Please report security problems privately through GitHub:
**Security ▸ Report a vulnerability** on <https://github.com/cpCr4zy/AIAssist-rust/security>.
Do not open a public issue for them.

Useful details: the version (`aiassist --version`), the platform, steps to reproduce and the impact.
You should get an answer within a week.

## Supported versions

Only the latest release gets security fixes.

## Scope notes

- The OpenRouter API key is kept in the OS keyring (or a `.api_key` file readable only by the user
  when no keyring is available); in the web build it is kept in the site's local storage.
- The app sends the text you submit, with the chosen prompt, to OpenRouter (`https://openrouter.ai`).
