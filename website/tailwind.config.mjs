/** Align with Zagens desktop light theme (`crates/desktop/web-ui/src/styles/globals.css`). */
export default {
  content: ['./src/**/*.{astro,html,js,jsx,md,mdx,svelte,ts,tsx,vue}'],
  theme: {
    extend: {
      colors: {
        canvas: '#faf9f7',
        'canvas-alt': '#f7f5f2',
        card: '#fdfcfb',
        'card-border': '#eae8e4',
        ink: '#1c1917',
        muted: '#57534e',
        subtle: '#a8a29e',
        accent: '#2563eb',
        'accent-hover': '#1d4ed8',
        'accent-soft': 'rgba(37, 99, 235, 0.08)',
        warning: '#d97706',
        'warning-bg': 'rgba(217, 119, 6, 0.08)',
      },
      fontFamily: {
        sans: [
          'Segoe UI',
          'system-ui',
          '-apple-system',
          'BlinkMacSystemFont',
          'Helvetica Neue',
          'Arial',
          'sans-serif',
        ],
        mono: ['Cascadia Code', 'Consolas', 'ui-monospace', 'monospace'],
      },
      boxShadow: {
        card: '0 8px 24px rgba(41, 37, 36, 0.05)',
      },
    },
  },
};
