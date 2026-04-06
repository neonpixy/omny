import { defineConfig, presetWind, type Rule } from 'unocss';

// ── Throne-safe color-mix rules ──────────────────────────────────
// Clean, paren-free class names for color-mix().
// Pattern: {property}-mix-{color}-{percent}[-{blendTo}]

const mixProperties: Record<string, string> = {
  bg: 'background-color',
  border: 'border-color',
  text: 'color',
  ring: '--un-ring-color',
  divide: '--un-divide-color',
  outline: 'outline-color',
};

function colorMixRules(): Rule[] {
  return Object.entries(mixProperties).flatMap(([prefix, property]) => [
    // Blend to transparent: bg-mix-accent-10
    [
      new RegExp(`^${prefix}-mix-([a-z-]+)-(\\d+)$`),
      ([, color, pct]: string[]) => ({
        [property]: `color-mix(in srgb, var(--color-${color}) ${pct}%, transparent)`,
      }),
    ] as Rule,
    // Blend to another color: bg-mix-accent-10-surface
    [
      new RegExp(`^${prefix}-mix-([a-z-]+)-(\\d+)-([a-z-]+)$`),
      ([, color, pct, other]: string[]) => ({
        [property]: `color-mix(in srgb, var(--color-${color}) ${pct}%, var(--color-${other}))`,
      }),
    ] as Rule,
  ]);
}

export default defineConfig({
  presets: [presetWind()],
  rules: colorMixRules(),
  postprocess: (util) => {
    util.entries.forEach((entry) => {
      if (typeof entry[1] === 'string' && entry[1].startsWith('--')) {
        entry[1] = `var(${entry[1]})`;
      }
    });
  },
  theme: {
    maxWidth: {
      xs: '20rem',
      sm: '24rem',
      md: '28rem',
      lg: '32rem',
      xl: '36rem',
      '2xl': '42rem',
      '3xl': '48rem',
      '4xl': '56rem',
      '5xl': '64rem',
      '6xl': '72rem',
      '7xl': '80rem',
    },
  },
  content: {
    filesystem: [
      'src/**/*.{tsx,ts,css}',
      // Scan Apps source for utility classes
      '../../Apps/**/*.{tsx,ts,css}',
      // Scan @omnidea/ui source
      '../../Library/ui/src/**/*.{tsx,ts,css}',
    ],
  },
});
