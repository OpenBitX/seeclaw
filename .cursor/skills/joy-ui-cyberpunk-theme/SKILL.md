---
name: joy-ui-neutral-dual-theme
description: Apply the SeeClaw neutral dual theme (light + dark) using Joy UI. Use when setting up the global theme, styling components, implementing streaming reasoning UI, building action cards, or when asked to "style the UI", "set up the theme", "fix theme", or "add action card".
argument-hint: <component-file-or-feature>
---

# Joy UI Theme — Neutral Dual Theme (OpenAI-style)

## Design Principles

- **Clean, minimal, restrained** — inspired by Claude / ChatGPT
- Palette: gray, black, white, beige/cream
- **NO gradients** (`linear-gradient` / `radial-gradient` forbidden)
- **NO glassmorphism abuse** — `backdrop-filter: blur()` ONLY for fullscreen modals
- Light and dark mode fully supported via `CssVarsProvider`

## 1. Global Theme Setup

Theme lives in `src-ui/src/theme/index.ts`. Never scatter theme values in components.

```ts
// src-ui/src/theme/index.ts
import { extendTheme } from '@mui/joy/styles';

export const theme = extendTheme({
  colorSchemes: {
    light: {
      palette: {
        primary: {
          50: '#f0f0f0', 100: '#e0e0e0', 200: '#c0c0c0',
          300: '#a0a0a0', 400: '#707070', 500: '#4a4a4a',
          600: '#333333', 700: '#1a1a1a',
          solidBg: '#1a1a1a', solidColor: '#ffffff',
          outlinedBorder: '#d0d0d0', outlinedColor: '#1a1a1a',
        },
        neutral: {
          50: '#fafafa', 100: '#f5f5f5', 200: '#eeeeee',
          300: '#e0e0e0', 400: '#bdbdbd', 500: '#9e9e9e',
          600: '#757575', 700: '#616161', 800: '#424242', 900: '#212121',
        },
        danger: {
          500: '#d32f2f', solidBg: '#d32f2f', solidColor: '#ffffff',
        },
        success: {
          500: '#2e7d32', solidBg: '#2e7d32', solidColor: '#ffffff',
        },
        background: {
          body: '#ffffff',
          surface: '#fafafa',
          popup: '#ffffff',
          level1: '#f5f5f0',  // slight warm beige
          level2: '#efefea',
          level3: '#e8e8e3',
        },
        text: {
          primary: '#1a1a1a',
          secondary: '#6b6b6b',
          tertiary: '#9e9e9e',
        },
        divider: 'rgba(0,0,0,0.08)',
      },
    },
    dark: {
      palette: {
        primary: {
          50: '#e8e8e8', 100: '#d0d0d0', 200: '#b0b0b0',
          300: '#909090', 400: '#787878', 500: '#a0a0a0',
          600: '#c0c0c0', 700: '#e0e0e0',
          solidBg: '#e8e8e8', solidColor: '#1a1a1a',
          outlinedBorder: '#404040', outlinedColor: '#e0e0e0',
        },
        neutral: {
          50: '#fafafa', 100: '#e0e0e0', 200: '#a0a0a0',
          300: '#707070', 400: '#505050', 500: '#404040',
          600: '#303030', 700: '#252525', 800: '#1a1a1a', 900: '#0f0f0f',
        },
        danger: {
          500: '#ef5350', solidBg: '#ef5350', solidColor: '#ffffff',
        },
        success: {
          500: '#66bb6a', solidBg: '#66bb6a', solidColor: '#000000',
        },
        background: {
          body: '#171717',
          surface: '#1e1e1e',
          popup: '#252525',
          level1: '#212121',
          level2: '#2a2a2a',
          level3: '#333333',
        },
        text: {
          primary: '#ececec',
          secondary: '#a0a0a0',
          tertiary: '#6b6b6b',
        },
        divider: 'rgba(255,255,255,0.08)',
      },
    },
  },
  radius: {
    xs: '4px', sm: '8px', md: '12px', lg: '16px', xl: '20px',
  },
  fontFamily: {
    body: '"Inter", "Segoe UI", system-ui, sans-serif',
    display: '"Inter", system-ui, sans-serif',
    code: '"JetBrains Mono", "Cascadia Code", "Fira Code", monospace',
  },
  fontSize: {
    xs: '0.75rem', sm: '0.8125rem', md: '0.875rem',
    lg: '1rem', xl: '1.125rem', xl2: '1.25rem',
  },
  components: {
    JoySheet: {
      styleOverrides: {
        root: ({ theme }) => ({
          backgroundColor: theme.vars.palette.background.surface,
          backdropFilter: 'none',
        }),
      },
    },
    JoyCard: {
      styleOverrides: {
        root: ({ theme }) => ({
          border: `1px solid ${theme.vars.palette.divider}`,
          boxShadow: 'none',
          backdropFilter: 'none',
        }),
      },
    },
    JoyButton: {
      styleOverrides: {
        root: {
          fontWeight: 600,
          letterSpacing: '0.01em',
          fontSize: '0.8125rem',
        },
      },
    },
    JoyInput: {
      styleOverrides: {
        root: ({ theme }) => ({
          '--Input-focusedHighlight': theme.vars.palette.primary[400],
        }),
      },
    },
  },
});
```

## 2. Glassmorphism Policy

**ALLOWED (sparingly):** `backdrop-filter: blur(8px)` ONLY for:
- Fullscreen modal overlays
- Floating command palette (Cmd+K style)

**FORBIDDEN everywhere else:**
- Card surfaces → solid `background.surface`
- Sidebars, panels → always solid
- Chat bubbles, message cards → always solid

```tsx
// ✅ Correct — solid background card
<Card variant="outlined" sx={{ bgcolor: 'background.level1' }}>

// ❌ Wrong — frosted glass
<Card sx={{ backdropFilter: 'blur(12px)', bgcolor: 'rgba(20,20,31,0.4)' }}>

// ❌ Wrong — gradient
<Box sx={{ background: 'linear-gradient(135deg, #000, #333)' }}>
```

## 3. Streaming Reasoning — Collapsible Accordion

```tsx
// Collapsible reasoning block — clean, minimal style
function ReasoningBlock({ text, isStreaming }: { text: string; isStreaming: boolean }) {
  const [open, setOpen] = React.useState(false);
  const lastLine = text.split('\n').filter(Boolean).at(-1) ?? '';

  return (
    <Box sx={{ my: 1 }}>
      <Box
        onClick={() => setOpen(!open)}
        sx={{
          display: 'flex', alignItems: 'center', gap: 1, cursor: 'pointer',
          color: 'text.tertiary', fontSize: 'xs', userSelect: 'none',
          '&:hover': { color: 'text.secondary' },
        }}
      >
        <Typography level="body-xs" sx={{ fontFamily: 'code' }}>
          {open ? '▼ thinking' : '▶ thinking'}
        </Typography>
        {!open && isStreaming && (
          <Typography
            level="body-xs"
            sx={{
              fontFamily: 'code', color: 'text.secondary',
              maxWidth: '60ch', overflow: 'hidden',
              textOverflow: 'ellipsis', whiteSpace: 'nowrap',
            }}
          >
            {lastLine}
          </Typography>
        )}
      </Box>
      {open && (
        <Box sx={{
          mt: 0.5, p: 1.5,
          bgcolor: 'background.level1',
          border: '1px solid', borderColor: 'divider',
          borderRadius: 'sm',
          fontFamily: 'code', fontSize: 'xs', color: 'text.secondary',
          whiteSpace: 'pre-wrap', wordBreak: 'break-word',
        }}>
          {text}
        </Box>
      )}
    </Box>
  );
}
```

## 4. Tool Call Action Cards

```tsx
import type { ColorPaletteProp } from '@mui/joy';

interface ActionMeta {
  icon: string;
  color: ColorPaletteProp;
  label: string;
}

const ACTION_META: Record<string, ActionMeta> = {
  MOUSE_CLICK:    { icon: '🖱', color: 'primary',  label: 'Mouse Click' },
  TERMINAL_CMD:   { icon: '⚡', color: 'danger',   label: 'Terminal Command' },
  KEYBOARD_INPUT: { icon: '⌨', color: 'neutral',  label: 'Keyboard Input' },
  SCREENSHOT:     { icon: '📷', color: 'success',  label: 'Screenshot' },
};

function ActionCard({ action, onApprove, onReject }: ActionCardProps) {
  const meta = ACTION_META[action.type] ?? { icon: '?', color: 'neutral' as const, label: action.type };
  const isDangerous = action.type === 'TERMINAL_CMD';

  return (
    <Card
      variant="outlined"
      sx={{ my: 1, p: 1.5, bgcolor: 'background.level1' }}
    >
      <Box sx={{ display: 'flex', alignItems: 'center', gap: 1, mb: isDangerous ? 1 : 0 }}>
        <Typography sx={{ fontSize: 'md' }}>{meta.icon}</Typography>
        <Typography level="title-sm" sx={{ fontFamily: 'code' }}>
          {meta.label}
        </Typography>
        <Box sx={{ flex: 1 }} />
        <Chip size="sm" variant="soft" color={meta.color}>{action.type}</Chip>
      </Box>

      <Typography level="body-xs" sx={{
        fontFamily: 'code', color: 'text.secondary', whiteSpace: 'pre-wrap',
      }}>
        {typeof action.payload === 'string' ? action.payload : JSON.stringify(action.payload, null, 2)}
      </Typography>

      {isDangerous && (
        <Box sx={{ display: 'flex', gap: 1, mt: 1 }}>
          <Button size="sm" color="success" variant="solid" onClick={onApprove}>Allow</Button>
          <Button size="sm" color="danger" variant="outlined" onClick={onReject}>Reject</Button>
        </Box>
      )}
    </Card>
  );
}
```

## 5. Typography & Spacing Rules

- Use `level` prop: `body-xs`, `body-sm`, `body-md`, `title-sm`, `title-md`, `h4`
- Code / LLM output → `fontFamily: 'code'`
- UI labels → `fontFamily: 'body'`
- Never set raw `fontSize` in px — use theme tokens only
- Spacing: prefer `gap`, `p`, `m` with theme multipliers
- **No magic numbers**: all values reference theme tokens or named constants
