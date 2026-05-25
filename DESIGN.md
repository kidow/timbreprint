# DESIGN.md

## Design Reference

This project uses the Linear design analysis from getdesign.md as the primary
reference, adapted for Timbreprint's local desktop workflow.

Source reference:
- https://getdesign.md/linear.app/design-md

The goal is not to clone Linear. The goal is to borrow the useful parts:
precision, low-noise hierarchy, dark product surfaces, hairline borders, compact
spacing, and a single restrained accent.

## Product Register

Timbreprint is a product UI, not a marketing site.

Design serves the workflow:
1. Select or import an audio source.
2. Run local analysis.
3. Inspect structured tags and confidence.
4. Copy an English prompt or open the job output.

The interface should feel like a focused desktop workbench: quiet, technical,
fast to scan, and trustworthy.

## Scene

A producer or developer is reviewing a local audio file on a laptop or desktop
monitor, likely in a dim workspace, comparing analysis output against the source
track. The UI should reduce ambient noise and make the current job state obvious.

This scene supports a dark product theme with restrained contrast.

## Principles

- Keep the surface dense but readable.
- Prefer panels, rows, and split panes over decorative cards.
- Use one primary accent only for action, focus, and active state.
- Do not use color as decoration.
- Make file state, run state, and output state visible without adding settings
  noise.
- Results should read from summary to tags to prompt to raw JSON.
- Empty states should show the next action, not explain the product.
- Every control needs a clear disabled and loading state.

## Color System

Use OKLCH or tinted HSL values in implementation. Avoid pure black and pure
white.

Recommended tokens:

```css
:root {
  --canvas: oklch(0.145 0.006 265);
  --surface-1: oklch(0.18 0.007 265);
  --surface-2: oklch(0.215 0.008 265);
  --surface-3: oklch(0.255 0.009 265);
  --border: oklch(0.31 0.011 265);
  --border-strong: oklch(0.39 0.013 265);
  --text: oklch(0.93 0.006 265);
  --text-muted: oklch(0.72 0.012 265);
  --text-subtle: oklch(0.58 0.014 265);
  --accent: oklch(0.64 0.15 285);
  --accent-hover: oklch(0.7 0.14 285);
  --success: oklch(0.68 0.16 150);
  --danger: oklch(0.66 0.18 28);
}
```

Color usage:
- `canvas`: app background.
- `surface-1`: main panels.
- `surface-2`: nested rows, selected states, empty states.
- `surface-3`: hover and pressed surface states.
- `border`: normal hairline.
- `border-strong`: focus or active border.
- `accent`: primary action, focus ring, active status only.
- `success` and `danger`: completed and failed states only.

Avoid:
- Decorative purple fills.
- Purple gradients.
- Bright inactive badges.
- Blue-gray monochrome without a purposeful accent.

## Typography

Use one system sans stack:

```css
font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont,
  "Segoe UI", sans-serif;
```

Scale:
- Page title: 28px to 34px, weight 650, line-height 1.08.
- Panel title: 16px to 18px, weight 650, line-height 1.25.
- Body: 14px to 15px, weight 400, line-height 1.55.
- Label: 12px, weight 650, uppercase only for short metadata labels.
- Code and paths: 12px to 13px mono, line-height 1.5.

Rules:
- Keep letter spacing at 0 except short uppercase metadata labels.
- Do not use display typography inside panels.
- Long prompts should be comfortable prose, capped around 70ch when possible.

## Layout

Primary layout:
- Top status bar.
- Left control rail for source selection and job state.
- Right result surface for analysis output.

Desktop:
- Left rail: 320px to 380px.
- Right surface: fills remaining width.
- Sticky left rail is acceptable on tall result pages.

Mobile or narrow desktop:
- Stack top bar, input, job state, result surface.
- Do not shrink text fluidly.
- Preserve stable button heights.

Panel rules:
- Border radius: 6px to 8px.
- Use hairline borders.
- Use shadows sparingly. Prefer border and surface contrast.
- Do not nest cards inside cards.
- Do not repeat equal card grids unless the data truly has equal weight.

## Components

### Buttons

Primary:
- Used only for `Analyze`.
- Accent background.
- Must support loading and disabled states.

Secondary:
- Used for file selection, copy, and open folder.
- Neutral surface or outline.

Icon buttons:
- Use lucide icons.
- Provide `aria-label` and `title`.

Loading:
- Button shows a spinner immediately on click.
- Disable while the analysis job is running.
- Keep button width stable enough that text does not jump.

### Badges

Use for compact state:
- File loaded.
- No file selected.
- Completed.
- Failed.
- Prompt ready.

Rules:
- Badges are low contrast by default.
- Success and failure may use semantic color.
- Confidence should remain human readable: low, medium, high.

### Panels

Use panels for:
- Input controls.
- Current job metadata.
- Analysis summary.
- Tags.
- Prompt.
- Raw JSON.

Panels should be quiet. Their job is to create scan zones, not decoration.

### Data Blocks

Paths, JSON, logs, and generated prompt text need:
- `overflow-wrap: anywhere` for paths.
- Scrollable JSON region with stable max height.
- Mono font for raw data only.

## State Design

Required states:
- `idle`: no file selected.
- `selected`: source loaded, analysis available.
- `preprocessing`: ffmpeg conversion running.
- `analyzing`: worker running.
- `prompting`: prompt generation running.
- `completed`: result available.
- `failed`: error shown inline.
- `cancelled`: reserved for future cancel support.

State visibility:
- Main status badge in the top bar.
- Button loading state during active work.
- Inline error near the action that failed.

## Motion

Use motion only for state feedback.

Allowed:
- Spinner during active work.
- 150ms to 200ms hover or focus transitions.
- Subtle opacity or color transitions.

Avoid:
- Page load choreography.
- Decorative motion.
- Layout animation.

## Copy

Use short operational labels.

Good:
- `Analyze`
- `Copy prompt`
- `Open output`
- `No file selected`
- `Prompt ready`

Avoid:
- Repeating what the user can already see.
- Marketing copy.
- Long instructions inside the main interface.

The UI may be bilingual while the generated prompt remains English.

## Accessibility

- Every icon-only control needs an accessible label.
- Focus states must be visible on dark surfaces.
- Disabled controls must still have readable text.
- Do not rely on color alone for status.
- Keep minimum touch target near 40px.

## Anti-Patterns

Do not introduce:
- Hero sections.
- Decorative gradients.
- Gradient text.
- Glass panels.
- Purple-heavy surfaces.
- Large marketing cards.
- Dense identical card grids.
- Settings panels that do not serve the current workflow.
- Modals for routine input.

## Implementation Notes

The current React app should continue using local components in
`src/components/ui`. Keep component APIs small and predictable.

When adjusting `src/styles.css`, prefer token updates and targeted component
rules over broad rewrites.

When adding future YouTube import support, it should appear as a second source
input mode in the left rail, not as a modal.
