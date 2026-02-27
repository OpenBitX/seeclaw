import { extendTheme } from '@mui/joy/styles';

export const theme = extendTheme({
  colorSchemes: {
    light: {
      palette: {
        primary: {
          50: '#f0f0f0',
          100: '#e0e0e0',
          200: '#c0c0c0',
          300: '#a0a0a0',
          400: '#808080',
          500: '#606060',
          600: '#404040',
          700: '#303030',
          800: '#202020',
          900: '#101010',
        },
        background: {
          body: '#F1EDE5',    // warm parchment — main content area
          surface: '#FDFDFD', // near-white — header / elevated surfaces
          popup: '#FDFDFD',   // dialogs
          level1: '#EAE6DE',
          level2: '#E3DFD7',
          level3: '#DBD7CF',
        },
        text: {
          primary: '#1a1a1a',
          secondary: '#5a5a5a',
          tertiary: '#8a8a8a',
        },
        neutral: {
          50: '#fafaf9',
          100: '#f0efec',
          200: '#e0deda',
          300: '#c8c6c2',
          400: '#a8a6a2',
          500: '#888582',
          600: '#686562',
          700: '#484542',
          800: '#282522',
          900: '#181512',
          outlinedBorder: '#d0cec8',
        },
        danger: {
          500: '#dc2626',
          600: '#b91c1c',
        },
        success: {
          500: '#16a34a',
          600: '#15803d',
        },
        warning: {
          500: '#d97706',
          600: '#b45309',
        },
      },
    },
    dark: {
      palette: {
        primary: {
          50: '#101010',
          100: '#202020',
          200: '#303030',
          300: '#404040',
          400: '#606060',
          500: '#909090',
          600: '#b0b0b0',
          700: '#c8c8c8',
          800: '#e0e0e0',
          900: '#f0f0f0',
        },
        background: {
          body: '#0f0f0f',
          surface: '#1a1a1a',
          popup: '#222222',
          level1: '#232323',
          level2: '#2a2a2a',
          level3: '#323232',
        },
        text: {
          primary: '#ececec',
          secondary: '#a0a0a0',
          tertiary: '#606060',
        },
        neutral: {
          50: '#181818',
          100: '#222222',
          200: '#2a2a2a',
          300: '#383838',
          400: '#505050',
          500: '#686868',
          600: '#848484',
          700: '#a0a0a0',
          800: '#c0c0c0',
          900: '#e0e0e0',
          outlinedBorder: '#383838',
        },
        danger: {
          500: '#f87171',
          600: '#ef4444',
        },
        success: {
          500: '#4ade80',
          600: '#22c55e',
        },
        warning: {
          500: '#fbbf24',
          600: '#f59e0b',
        },
      },
    },
  },
  fontFamily: {
    body: '"Inter", "Noto Sans SC", system-ui, -apple-system, sans-serif',
    display: '"Inter", "Noto Sans SC", system-ui, -apple-system, sans-serif',
    code: '"JetBrains Mono", "Fira Code", monospace',
  },
  fontSize: {
    xs: '0.75rem',
    sm: '0.875rem',
    md: '1rem',
    lg: '1.125rem',
    xl: '1.25rem',
    xl2: '1.5rem',
    xl3: '1.875rem',
    xl4: '2.25rem',
  },
  radius: {
    xs: '4px',
    sm: '6px',
    md: '8px',
    lg: '12px',
    xl: '16px',
  },
  components: {
    JoyButton: {
      defaultProps: {
        variant: 'solid',
        size: 'sm',
      },
    },
    JoyInput: {
      defaultProps: {
        variant: 'outlined',
        size: 'sm',
      },
    },
    JoyCard: {
      styleOverrides: {
        root: {
          borderRadius: '8px',
          boxShadow: 'none',
        },
      },
    },
    // Ensure Select dropdowns always float above modals/dialogs
    JoySelect: {
      defaultProps: {
        slotProps: {
          listbox: { sx: { zIndex: 9999 } },
        },
      },
    },
  },
});

export type SeeclawTheme = typeof theme;
