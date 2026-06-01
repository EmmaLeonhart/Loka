# Loka Windows installer

This directory builds `loka-setup-x64.exe` with [Inno Setup 6](https://jrsoftware.org/isinfo.php).

- **`loka.iss`** — the Inno Setup script (the installer itself).
- **`models.toml`** — declares the inference model(s) the installer can offer.

## What the installer does

The installer drives one choice: install the **database alone**, or the
**database plus an inference model**. Inno Setup `[Types]` expose this as:

| Type | Installs |
|---|---|
| `engine_only` | `loka.exe` + Loka Studio (the engine is the `engine` component, `Flags: fixed`) |
| `engine_model` | the above **plus** the `model` component |

The engine component always ships `loka.exe`, `Loka Studio.exe`, `models.toml`,
`LICENSE`, and `README.md` into the install dir. It optionally adds Loka to the
system PATH and creates shortcuts.

Selecting the model component does **not** bundle weights into the installer.
On `ssPostInstall` the script writes an `install-selection.toml` manifest into
the install dir, which `loka.exe` reads on first launch to decide whether to
pull the model from Hugging Face. `install-selection.toml` looks like:

```toml
# Written by the Loka installer. Read by loka.exe on first launch
# to decide whether to pull an inference model from Hugging Face.

install_model = true
model_id      = "qwen-2.5-1.5b-instruct"
model_repo    = "Qwen/Qwen2.5-1.5B-Instruct"
```

(When the model component is deselected: `install_model = false` and the two
string fields are empty.)

## `models.toml` schema

Each model is a `[[model]]` array-of-tables entry. **Only the first entry is
used today** — see "Current limitation" below.

| Field | Type | Meaning |
|---|---|---|
| `id` | string | Stable identifier written into `install-selection.toml` as `model_id`. |
| `display_name` | string | Human-readable name shown in the installer UI. |
| `hf_repo` | string | Hugging Face repo the weights are pulled from (written as `model_repo`). |
| `approx_size` | string | On-disk size shown in the UI (e.g. `"3.0 GB"`). |
| `fetch_mode` | string | `"first-run"` defers the download to the first inference call so the installer stays small. `"bundle"` (inline the weights into the installer) is **not implemented**. |
| `description` | string (multi-line) | Longer prose describing the model. |

The shipping default is Qwen 2.5 1.5B Instruct with `fetch_mode = "first-run"`:
combined with Loka's BFS + embedding retrieval over the local triplestore, that
is the inference path the project actually ships (no fine-tune).

## How `loka.iss` consumes `models.toml`

Inno Setup **cannot parse TOML at runtime**, so `loka.iss` does not read
`models.toml` during installation. Instead the model fields are mirrored into
compile-time `#define`s at the top of `loka.iss`:

```iss
#define ModelId      "qwen-2.5-1.5b-instruct"
#define ModelDisplay "Qwen 2.5 1.5B Instruct"
#define ModelRepo    "Qwen/Qwen2.5-1.5B-Instruct"
#define ModelSize    "3.0 GB"
```

These mirror the **first** `[[model]]` in `models.toml`. `models.toml` itself is
still copied into the install dir (so `loka.exe` and the user can see the full
declaration), but the wizard text and `install-selection.toml` come from the
`#define`s.

**Keep the two in sync by hand.** CI (`.github/workflows/release.yml`) invokes
`ISCC.exe` with only `/DLokaVersion`, `/DLokaBinary`, and `/DLokaStudio` — it
does **not** override the model `#define`s — so the `loka.iss` defaults are what
ship. If you change the first model in `models.toml`, update the four
`Model*` `#define`s to match. (Each can also be overridden per-build with
`/DModelId=...` etc. if needed.)

## Building locally

```bat
ISCC.exe installer\loka.iss /DLokaVersion=0.4.0 /DLokaBinary=target\release\loka.exe
```

Output lands in `dist\installer\loka-setup-x64.exe`.

## Current limitation & roadmap

Only the first `[[model]]` is offered — `models.toml` is already shaped as an
array so additional entries are silently ignored until multi-model support
lands. The plan (one `[Components]` entry per model, mutually exclusive, with a
CI pre-step that generates the `.iss` include from `models.toml` since Inno
can't read TOML at runtime) is tracked under **"Windows installer — multi-model
support"** in `TODO.md`.
