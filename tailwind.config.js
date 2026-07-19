/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        // LocalKit design kit — the zinc scale is remapped to the navy-tinted
        // palette so existing classes pick up the brand surface colors.
        zinc: {
          50: "#F7F7FB",
          100: "#F1F2F7",
          200: "#E4E7EE",
          300: "#D3D7E2",
          400: "#B8BFD0",
          500: "#9097AB", // muted text
          600: "#6E7488", // dim text
          700: "#3A4056", // strong border
          800: "#2A2F40", // muted border
          900: "#151822", // surface
          950: "#0D0F16", // deep navy background
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
