# Skill: File Operations

## Metadata

**name**: `file_operations`

**description**: Provides file and folder operations on Windows including opening File Explorer, navigating directories, creating files/folders, and basic file management.

**example**: 
```
Task: Open Documents folder
Plan:
1. Press Win+E to open File Explorer
2. Wait for File Explorer to load
3. Click on "Documents" in Quick access
```

**rules**:
1. Use Win+E to open File Explorer directly
2. Use Win+L to lock screen when needed
3. For file operations, always verify file/folder exists before acting
4. Use clear_first=true when typing file paths to avoid errors
5. Wait for visual stability after file operations
6. Handle "File in use" errors gracefully
7. Use Ctrl+C for copy, Ctrl+V for paste, Ctrl+X for cut
8. Use Delete key for delete (with confirmation)

**role**: Use this skill when user asks to:
- Open File Explorer
- Navigate to a folder
- Create, copy, move, or delete files/folders
- Access file system
- Manage documents

## Implementation Steps

### Open File Explorer
- Action: `Hotkey` with keys "win+e"
- Purpose: Opens File Explorer
- Expected: File Explorer window appears

### Navigate to Folder
- Action: `TypeText` with text="{folder_path}" and clear_first=true
- Purpose: Navigate to specific folder
- Expected: Folder contents displayed

### Create New Folder
- Action: `Hotkey` with keys "ctrl+shift+n"
- Purpose: Create new folder
- Expected: New folder appears

### Copy File/Folder
- Action: `Hotkey` with keys "ctrl+c"
- Purpose: Copy selected item
- Expected: Item copied to clipboard

### Paste File/Folder
- Action: `Hotkey` with keys "ctrl+v"
- Purpose: Paste copied item
- Expected: Item appears in current location

## Example Plans

### Example 1: Open File Explorer
```json
{
  "steps": [
    {
      "description": "Open File Explorer with Win+E",
      "needs_viewport": false,
      "action": {
        "type": "hotkey",
        "keys": "win+e"
      }
    },
    {
      "description": "Wait for File Explorer to load",
      "needs_viewport": false,
      "action": {
        "type": "wait",
        "milliseconds": 1000
      }
    }
  ]
}
```

### Example 2: Navigate to Documents
```json
{
  "steps": [
    {
      "description": "Open File Explorer with Win+E",
      "needs_viewport": false,
      "action": {
        "type": "hotkey",
        "keys": "win+e"
      }
    },
    {
      "description": "Wait for File Explorer to load",
      "needs_viewport": false,
      "action": {
        "type": "wait",
        "milliseconds": 1000
      }
    },
    {
      "description": "Click on address bar",
      "needs_viewport": true,
      "target": "address bar",
      "action": {
        "type": "mouse_click",
        "element_id": ""
      }
    },
    {
      "description": "Type Documents folder path",
      "needs_viewport": false,
      "action": {
        "type": "type_text",
        "text": "C:\\Users\\{username}\\Documents",
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
      "description": "Wait for folder to load",
      "needs_viewport": false,
      "action": {
        "type": "wait",
        "milliseconds": 500
      }
    }
  ]
}
```

## Common Paths

- `C:\Users\{username}\Documents` - Documents folder
- `C:\Users\{username}\Desktop` - Desktop
- `C:\Users\{username}\Downloads` - Downloads
- `C:\Users\{username}\Pictures` - Pictures
- `C:\Users\{username}\Music` - Music
- `C:\Users\{username}\Videos` - Videos

## Notes

- Always use backslashes (\) for Windows paths
- Replace {username} with actual username
- File Explorer has Quick access on the left panel
- Use F2 to rename selected file/folder
- Use Alt+Enter to open properties dialog
