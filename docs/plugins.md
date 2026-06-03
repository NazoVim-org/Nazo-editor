# Plugin System

ijevim supports plugins written in **Lua**, **Lisp**, **JavaScript**, **Nix**, or compiled **Rust** `.so`/`.dylib`.

## Loading

At startup, `PluginManager::load_all()` does:

1. Loads `~/.config/ijevim/init.{lua,lisp,js,nix}` (first file per extension wins).
2. Loads every file in `~/.config/ijevim/plugins/`.
3. Calls `setup()` on each loaded plugin.
4. Emits `PluginEvent::Ready`.

## API Surface

Every plugin receives a **plugin API object** with three methods:

| Method | Purpose |
|--------|---------|
| `addCommand(name)` / `add-command` | Register a named command |
| `on(event)` | Register a handler for an event |
| `log(msg)` | Print a message to the plugin log |

## Events

| Event | Fired When |
|-------|------------|
| `"Ready"` | All plugins loaded, editor about to start |
| `"ModeChange"` | Editor mode changed (normal/insert/visual) |
| `"BufferChange"` | Buffer content changed |
| `"BufferSave"` | Buffer saved to disk |
| `"Key"` | A key was pressed |

## Language Guides

### Lua

File: `init.lua` or `*.lua` in `plugins/`

```lua
return {
  name = "my-plugin",
  version = "0.1.0",

  setup = function(api)
    api.addCommand("hello", function()
      api.log("Hello from Lua!")
    end)

    api.on("Ready", function()
      api.log("my-plugin ready")
    end)
  end
}
```

The Lua plugin must return a table with:
- `name` (string) — plugin name
- `version` (string, optional) — version string
- `setup` (function) — called with the API object

### Lisp

File: `init.lisp` or `*.lisp` in `plugins/`

```lisp
(add-command "hello")
(on "Ready")
(log "my-plugin loaded")
```

The Lisp environment extends the default `rust_lisp` env with three additional functions:
- `(add-command name)` — register a command
- `(on event)` — listen for an event
- `(log msg ...)` — log a message

### JavaScript

File: `init.js` or `*.js` in `plugins/`

```javascript
ijevim.addCommand("hello-js");
ijevim.on("Ready");
ijevim.log("my-plugin loaded");
```

A global `ijevim` object is available with:
- `ijevim.addCommand(name)` — register a command
- `ijevim.on(event)` — listen for an event
- `ijevim.log(msg)` — log a message

### Nix

File: `init.nix` or `*.nix` in `plugins/`

```nix
{
  name = "my-plugin";
  version = "0.1.0";
  description = "A Nix-based plugin";
}
```

Nix files are evaluated via `nix-instantiate --eval --json --strict`.
The result becomes the plugin config. If `repo_url` is set, the plugin
auto-clones the repository on `setup()`.

### Rust (.so / .dylib)

Compile a `cdylib` crate exporting these symbols:

```c
uint32_t ijevim_plugin_api_version();           // must return 1
PluginVtable* ijevim_plugin_vtable();
```

`PluginVtable`:

```c
typedef void (*PluginCmdCb)(const uint8_t* cmd, size_t len);
typedef void (*PluginEventCb)(uint32_t event_type, size_t data);

typedef struct {
    const char* (*get_name)();
    void (*setup)(PluginApiTable api);
    void (*handle_event)(uint32_t event_type, size_t data);
    uint8_t (*execute_command)(const uint8_t* cmd, size_t len);
} PluginVtable;
```

`PluginApiTable` passed to `setup()`:

```c
typedef struct {
    void* ctx;
    void (*log)(void* ctx, const uint8_t* msg, size_t len);
    void (*add_command)(void* ctx, const uint8_t* name, size_t len, PluginCmdCb cb);
    void (*on)(void* ctx, const uint8_t* event, size_t len, PluginEventCb cb);
} PluginApiTable;
```

## Example Files

Example plugins for each language are in `src/plugins/`:

- `hello.lua`
- `hello.lisp`
- `hello.js`
- `hello.nix`
