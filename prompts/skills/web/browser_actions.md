# Skill: Browser Actions

## Metadata

**name**: `browser_actions`

**description**: Provides web browser navigation and interaction actions including opening URLs, navigating back/forward, refreshing pages, and managing tabs.

**example**: 
```
Task: Open Google
Plan:
1. Open browser (Chrome/Edge)
2. Type "google.com" in address bar
3. Press Enter to navigate
4. Wait for page to load
```

**rules**:
1. Use Ctrl+T to open new tab
2. Use Ctrl+W to close current tab
3. Use Ctrl+L to focus address bar
4. Use Alt+Left for back, Alt+Right for forward
5. Use F5 or Ctrl+R to refresh
6. Use Ctrl+D to bookmark current page
7. Wait for visual stability after navigation
8. Use clear_first=true when typing URLs

**role**: Use this skill when user asks to:
- Open a website
- Navigate to a URL
- Go back/forward in browser
- Refresh a page
- Open/close browser tabs
- Bookmark a page

## Implementation Steps

### Open Browser
- Action: Use open_software skill to launch browser
- Purpose: Opens Chrome or Edge
- Expected: Browser window appears

### Focus Address Bar
- Action: `Hotkey` with keys "ctrl+l"
- Purpose: Focus on URL input
- Expected: Address bar is highlighted

### Type URL
- Action: `TypeText` with text="{url}" and clear_first=true
- Purpose: Enter website address
- Expected: URL appears in address bar

### Navigate
- Action: `KeyPress` with key="enter"
- Purpose: Navigate to URL
- Expected: Page loads

### Wait for Page Load
- Action: `Wait` with milliseconds=2000
- Purpose: Allow page to fully load
- Expected: Page content is visible

## Example Plans

### Example 1: Open Google
```json
{
  "steps": [
    {
      "description": "Open Chrome browser",
      "needs_viewport": false,
      "action": {
        "type": "hotkey",
        "keys": "win+s"
      }
    },
    {
      "description": "Type 'Google Chrome'",
      "needs_viewport": false,
      "action": {
        "type": "type_text",
        "text": "Google Chrome",
        "clear_first": true
      }
    },
    {
      "description": "Wait for search results",
      "needs_viewport": false,
      "action": {
        "type": "wait",
        "milliseconds": 500
      }
    },
    {
      "description": "Press Enter to open Chrome",
      "needs_viewport": false,
      "action": {
        "type": "key_press",
        "key": "enter"
      }
    },
    {
      "description": "Wait for Chrome to load",
      "needs_viewport": false,
      "action": {
        "type": "wait",
        "milliseconds": 2000
      }
    },
    {
      "description": "Focus address bar with Ctrl+L",
      "needs_viewport": false,
      "action": {
        "type": "hotkey",
        "keys": "ctrl+l"
      }
    },
    {
      "description": "Type Google URL",
      "needs_viewport": false,
      "action": {
        "type": "type_text",
        "text": "https://www.google.com",
        "clear_first": true
      }
    },
    {
      "description": "Press Enter to navigate",
      "needs_viewport": false,
      "action": {
        "type": "key_press",
        "key": "enter"
      }
    },
    {
      "description": "Wait for page to load",
      "needs_viewport": false,
      "action": {
        "type": "wait",
        "milliseconds": 2000
      }
    }
  ]
}
```

### Example 2: Open New Tab
```json
{
  "steps": [
    {
      "description": "Open new tab with Ctrl+T",
      "needs_viewport": false,
      "action": {
        "type": "hotkey",
        "keys": "ctrl+t"
      }
    },
    {
      "description": "Wait for new tab",
      "needs_viewport": false,
      "action": {
        "type": "wait",
        "milliseconds": 500
      }
    },
    {
      "description": "Focus address bar",
      "needs_viewport": false,
      "action": {
        "type": "hotkey",
        "keys": "ctrl+l"
      }
    },
    {
      "description": "Type URL",
      "needs_viewport": false,
      "action": {
        "type": "type_text",
        "text": "https://example.com",
        "clear_first": true
      }
    },
    {
      "description": "Press Enter to navigate",
      "needs_viewport": false,
      "action": {
        "type": "key_press",
        "key": "enter"
      }
    },
    {
      "description": "Wait for page to load",
      "needs_viewport": false,
      "action": {
        "type": "wait",
        "milliseconds": 2000
      }
    }
  ]
}
```

## Browser Shortcuts

- `Ctrl+T` - New tab
- `Ctrl+W` - Close current tab
- `Ctrl+L` - Focus address bar
- `Alt+Left` - Go back
- `Alt+Right` - Go forward
- `F5` or `Ctrl+R` - Refresh page
- `Ctrl+D` - Bookmark page
- `Ctrl+Shift+T` - Reopen last closed tab
- `Ctrl+Tab` - Next tab
- `Ctrl+Shift+Tab` - Previous tab

## Notes

- Works with Chrome, Edge, and Firefox
- Visual stability detection ensures page is fully loaded
- Address bar automatically adds http:// if missing
- Use https:// for secure sites
