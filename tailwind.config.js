/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        // LocalKit design kit — the zinc scale resolves to the CSS-var token
        // layer in index.css (plan 28), so every zinc-* class is theme-aware:
        // dark keeps the navy-tinted palette, light inverts the ramp. The
        // semantic roles are stable: 950 page bg, 900 surface, 800/700
        // borders, 600/500 dim/muted text, 50 strongest text.
        zinc: {
          50: "rgb(var(--c-zinc-50) / <alpha-value>)",
          100: "rgb(var(--c-zinc-100) / <alpha-value>)",
          200: "rgb(var(--c-zinc-200) / <alpha-value>)",
          300: "rgb(var(--c-zinc-300) / <alpha-value>)",
          400: "rgb(var(--c-zinc-400) / <alpha-value>)",
          500: "rgb(var(--c-zinc-500) / <alpha-value>)",
          600: "rgb(var(--c-zinc-600) / <alpha-value>)",
          700: "rgb(var(--c-zinc-700) / <alpha-value>)",
          800: "rgb(var(--c-zinc-800) / <alpha-value>)",
          900: "rgb(var(--c-zinc-900) / <alpha-value>)",
          950: "rgb(var(--c-zinc-950) / <alpha-value>)",
        },
        // Brand violet per design kit (#6C5CE7) + soft lavender accent.
        violet: {
          300: "#CDC4FB",
          400: "#B8AFFA", // soft lavender (accents, links)
          500: "#7A6BEA", // primary hover
          600: "#6C5CE7", // primary
          700: "#5B4BD8",
          800: "#4638A8",
          900: "#352A80",
          950: "#221C4A",
        },
      },
      borderRadius: {
        md: "8px", // small controls
        lg: "10px", // inputs
        xl: "12px", // cards
        "2xl": "16px", // panels
        "3xl": "20px", // large surfaces
      },
      fontFamily: {
        sans: [
          "Inter",
          "-apple-system",
          "BlinkMacSystemFont",
          '"Segoe UI"',
          "Roboto",
          "system-ui",
          "sans-serif",
        ],
        mono: ['"JetBrains Mono"', '"Cascadia Code"', "Consolas", "monospace"],
      },
      boxShadow: {
        panel:
          "0 12px 32px rgba(0, 0, 0, 0.28), 0 0 0 1px rgba(255, 255, 255, 0.03)",
      },
    },
  },
  plugins: [],
};
