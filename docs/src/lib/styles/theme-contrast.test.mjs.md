# Theme Contrast Test

Source: `src/lib/styles/theme-contrast.test.mjs`.

## Helpers

- `variable(name)` — reads a hex custom property from production CSS.
- `luminance(hex)` — converts sRGB to relative luminance.
- `contrast(left,right)` — WCAG contrast ratio.

Case: muted-dim text must meet WCAG AA (`>=4.5`) on every authored surface, including selection and hover.

Target: [`theme.css`](theme.css.md).
