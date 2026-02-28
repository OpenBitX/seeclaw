# Skill: Open Software

## Metadata

**name**: `open_software`

**description**: Opens software applications on Windows using the system search (Win+S). This skill provides a reliable method to launch applications by searching for them in the Windows Start menu and then opening them.

**example**: 
```
Task: Open Notepad
Plan:
1. Press Win+S to open system search
2. Type "Notepad" in the search box
3. Wait for search results to appear
4. Press Enter to open Notepad
5. Wait for Notepad window to appear
```

**rules**:
1. Always use Win+S to open system search first
2. Type the software name exactly as it appears in the system
3. Wait at least 500ms after typing to allow search results to load
4. Press Enter to open the first search result
5. Wait for visual stability after opening to ensure the application is fully loaded
6. Handle cases where the application is already running (focus existing window)
7. Use TypeText with clear_first=true to ensure clean search input
8. If the application doesn't open after 3 seconds, consider it a failure

**role**: Use this skill when the user asks to:
- Open any software application (Notepad, Chrome, VS Code, etc.)
- Launch a program
- Start an application
- Run a specific software

## Implementation Steps

### Step 1: Open System Search
- Action: `Hotkey` with keys "win+s"
- Purpose: Opens Windows system search
- Expected: Search box appears

### Step 2: Type Software Name
- Action: `TypeText` with text="{software_name}" and clear_first=true
- Purpose: Search for the software
- Expected: Search results appear

### Step 3: Wait for Search Results
- Action: `Wait` with milliseconds=500
- Purpose: Allow search results to load
- Expected: Search results are visible

### Step 4: Open Application
- Action: `KeyPress` with key="enter"
- Purpose: Open the first search result
- Expected: Application launches

### Step 5: Wait for Application
- Action: `Wait` with milliseconds=1000
- Purpose: Allow application to fully load
- Expected: Application window is visible

## Common Software Names

Use these exact names for popular applications:
- `Notepad` - Windows Notepad
- `Calculator` - Windows Calculator
- `Microsoft Edge` - Edge browser
- `Google Chrome` - Chrome browser
- `Visual Studio Code` - VS Code editor
- `Microsoft Word` - Word processor
- `Microsoft Excel` - Spreadsheet application
- `File Explorer` - File manager
- `Windows Terminal` - Command line interface
- `Task Manager` - System task manager

## Error Handling

If the application doesn't open:
1. Check if the software name is spelled correctly
2. Try alternative names (e.g., "Chrome" instead of "Google Chrome")
3. Verify the software is installed
4. Report the failure to the user with specific error details

## Example Plans

### Example 1: Open Notepad
```json
{
  "steps": [
    {
      "description": "Open system search with Win+S",
      "needs_viewport": false,
      "action": {
        "type": "hotkey",
        "keys": "win+s"
      }
    },
    {
      "description": "Type 'Notepad' in search box",
      "needs_viewport": false,
      "action": {
        "type": "type_text",
        "text": "Notepad",
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
      "description": "Press Enter to open Notepad",
      "needs_viewport": false,
      "action": {
        "type": "key_press",
        "key": "enter"
      }
    },
    {
      "description": "Wait for Notepad to load",
      "needs_viewport": false,
      "action": {
        "type": "wait",
        "milliseconds": 1000
      }
    }
  ]
}
```

### Example 2: Open Chrome
```json
{
  "steps": [
    {
      "description": "Open system search with Win+S",
      "needs_viewport": false,
      "action": {
        "type": "hotkey",
        "keys": "win+s"
      }
    },
    {
      "description": "Type 'Google Chrome' in search box",
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
    }
  ]
}
```

## Notes

- This skill works on Windows 10 and Windows 11
- The Win+S shortcut opens the system-wide search
- Search results are prioritized by usage frequency
- If multiple applications have the same name, the first result is opened
- Visual stability detection is automatically triggered after opening
