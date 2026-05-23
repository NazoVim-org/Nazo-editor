# Emacs Mark/Region System — Design Document

## 1. 概要

Emacs キーマップに Mark/Region システムを追加する設計。現在の ijevim の Emacs 互換性には、選択範囲操作（`C-w` kill-region、`M-w` copy-region）が欠けている。これを実現するために必要なデータ構造、API、キーバインドの変更を定義する。

### 現状のギャップ

| キー | 現在の動作 | 望ましい動作 |
|------|-----------|------------|
| `C-space` | 未対応 | マークをセットする |
| `C-w` | `kill_word()` | region があれば kill-region、なければ kill-word |
| `M-w` | 未対応 | region があれば copy-region |
| `C-y` | `undo()` | kill-ring から yank |
| `C-g` | command モードのみ abort | region の deactivate + abort |

---

## 2. データモデル

### 2.1 EditorState への追加フィールド

`src/types.rs` — `EditorState` 構造体に以下を追加:

```rust
pub struct EditorState {
    // ... existing fields ...
    pub mark: Option<Position>,       // C-space でセットされるマーク位置
    pub region_active: bool,          // リージョンがアクティブか
}
```

- `mark: Some(Position)` + `region_active: true` → リージョン定義中（ハイライト表示）
- `mark: Some(Position)` + `region_active: false` → マークはあるが非アクティブ（`C-g` 後の状態）
- `mark: None` → マーク未設定

**なぜ `visual_start` を再利用しないのか:**

`visual_start` は Vim Visual モードの開始位置であり、Vim モードと Emacs モードで意味が異なる。混在を避けるため、`mark` フィールドは独立させる。ただしレンダリングのリージョンハイライト表示は、Vim Visual モードと共通の仕組みを使う（後述）。

### 2.2 Kill-Ring

`src/editor.rs` または新規 `src/kill_ring.rs` に追加:

```rust
pub struct KillRing {
    entries: Vec<String>,
    index: usize,       // 現在の yank 位置（循環）
    max_entries: usize,  // デフォルト 60
}
```

**なぜ Register を再利用しないのか:**

- Emacs の kill-ring は **循環バッファ**であり、複数エントリを保持できる
- `C-y` の後の `M-y` (yank-pop) で循環的に過去のキルを辿る必要がある
- 現在の `Register` は単一文字 key → String の HashMap で、循環動作に適さない

KillRing の API:

```rust
impl KillRing {
    pub fn new() -> Self;
    pub fn push(&mut self, text: &str);         // 新しい kill を追加
    pub fn yank(&self) -> Option<&str>;         // 最新エントリを取得
    pub fn yank_pop(&mut self) -> Option<&str>; // 前のエントリに戻る（循環）
    pub fn reset_index(&mut self);              // yank-pop のインデックスをリセット
}
```

### 2.3 Editor への追加フィールド

```rust
pub struct Editor {
    // ... existing fields ...
    pub kill_ring: KillRing,
}
```

---

## 3. キーバインド変更

### 3.1 `C-space` — マークセット

**場所**: `src/keymap/emacs.rs`

```rust
(false, KeyCode::Char(' ')) if has_ctrl => {
    editor.set_mark();
}
```

**ロジック** (`Editor::set_mark`):

```rust
pub fn set_mark(&mut self) {
    self.state.mark = Some(self.state.cursor);
    self.state.region_active = true;
    // ステータスラインに "Mark set" を表示（次のキー入力で消える）
    self.needs_render = true;
}
```

### 3.2 `C-w` — Region kill / word kill（条件分岐）

**変更前**:
```rust
(true, KeyCode::Char('w')) => {
    editor.kill_word();
}
```

**変更後**:
```rust
(true, KeyCode::Char('w')) => {
    if editor.has_active_region() {
        editor.kill_region();
    } else {
        editor.kill_word();
    }
}
```

**`has_active_region()`**:
```rust
pub fn has_active_region(&self) -> bool {
    self.state.region_active
        && self.state.mark.is_some()
        && self.state.mark.unwrap() != self.state.cursor
}
```

### 3.3 `M-w` — Region コピー

**場所**: `src/keymap/emacs.rs`

```rust
(false, KeyCode::Char('w')) if has_alt => {
    if editor.has_active_region() {
        editor.copy_region();
    }
}
```

### 3.4 `C-y` — Yank（kill-ring からペースト）

**変更前**:
```rust
(true, KeyCode::Char('y')) => {
    editor.yank_pop();  // これは単なる undo()
}
```

**変更後**:
```rust
(true, KeyCode::Char('y')) => {
    if editor.has_active_region() {
        // リージョンがある場合はまずリージョンを削除
        editor.delete_region();
    }
    editor.yank();  // kill-ring からペースト
}
```

`yank_pop()` は `undo()` を呼んでいたが、これは元の `C-/` undo と重複している。新しい `C-y` では `KillRing::yank()` から最新エントリを挿入する。

**`:yank_pop()` の後方互換性**: 既存の `yank_pop()` (`undo()`) は削除する。その機能は `C-/` / `C-_` / `C-?` でカバーされている。

### 3.5 `M-y` — Yank-pop（kill-ring を遡る）

新規追加:

```rust
(false, KeyCode::Char('y')) if has_alt => {
    editor.yank_pop_from_kill_ring();
}
```

### 3.6 `C-g` — Region deactivate + abort

**変更前**:
```rust
(true, KeyCode::Char('g')) => {
    editor.abort();
}
```

**変更後**:
```rust
(true, KeyCode::Char('g')) => {
    editor.deactivate_region();
    editor.abort();
}
```

---

## 4. Region 操作の実装詳細

### 4.1 `kill_region()`

```rust
pub fn kill_region(&mut self) {
    if let Some(mark) = self.state.mark {
        let (s_line, s_col, e_line, e_col) = self.normalize_selection(&mark, &self.state.cursor);
        let content = self.buffer.get_char_range(s_line, s_col, e_line, e_col);
        if !content.is_empty() {
            self.buffer.delete_range(s_line, s_col, e_line, e_col);
            self.kill_ring.push(&content);
            self.register.set('"', &content);
            self.on_buffer_modified();
        }
        self.deactivate_region();
    }
}
```

**undo push**: 削除操作を undo スタックに追加（`EditType::Delete`）。

**注意**: `normalize_selection()` は既存の Vim Visual モード用メソッドを流用する（`src/editor.rs` L1322-1331）。

### 4.2 `copy_region()`

```rust
pub fn copy_region(&mut self) {
    if let Some(mark) = self.state.mark {
        let (s_line, s_col, e_line, e_col) = self.normalize_selection(&mark, &self.state.cursor);
        let content = self.buffer.get_char_range(s_line, s_col, e_line, e_col);
        if !content.is_empty() {
            self.kill_ring.push(&content);
            self.register.set('"', &content);
        }
        self.deactivate_region();
    }
}
```

### 4.3 `deactivate_region()`

```rust
pub fn deactivate_region(&mut self) {
    self.state.region_active = false;
    // mark は保持する（再度 C-g でクリアしてもよい）
    self.needs_render = true;
}
```

### 4.4 Kill-ring yank/pop

```rust
pub fn yank_from_kill_ring(&mut self) {
    if let Some(text) = self.kill_ring.yank() {
        self.buffer.insert(self.state.cursor.line, self.state.cursor.col, text);
        self.state.cursor.col += text.chars().count();
        self.state.dirty = true;
        self.needs_render = true;
    }
}

pub fn yank_pop_from_kill_ring(&mut self) {
    // M-y で kill-ring を遡る
    // まず前回の yank を元に戻してから、一つ前のエントリを挿入
    if let Some(text) = self.kill_ring.yank_pop() {
        // TODO: 前回の yank を undo してから挿入
        // 簡易実装: まず最後の文字を削除して再挿入
        // 完全実装には yank 履歴のトラッキングが必要
    }
}
```

**Yank-pop の課題**: 完全な yank-pop には、前回の yank 挿入位置と長さを追跡する必要がある。初期実装では `M-y` をスキップし、kill-ring からの `C-y` のみ実装する。

---

## 5. レンダリング（リージョンハイライト）

### 5.1 表示仕様

- `region_active == true` かつ `mark != cursor` のとき、選択範囲を反転表示
- Vim Visual モードと同じ視覚スタイルを使用する（背景色反転）
- ステータスラインにモード表示として `-- REGION --` または `"Mark set"` を表示
- マークセット直後（リージョンがまだ空）は `"Mark set"` を一瞬表示

### 5.2 実装場所

`src/renderer.rs` の `render()` メソッド内:

```rust
let region_range = if state.region_active {
    state.mark.and_then(|mark| {
        if mark != state.cursor {
            Some(normalize_selection(&mark, &state.cursor))
        } else {
            None
        }
    })
} else {
    None
};
```

- リージョン範囲が決定したら、該当行の該当カラム範囲を背景色反転で描画
- Vim Visual モード (`state.mode == Mode::Visual`) と同様のロジックを共用する

### 5.3 ステータスライン表示

```rust
// 現在の mode 表示部分
let mode_display = if state.region_active {
    if state.mark == Some(state.cursor) {
        "-- MARK --"
    } else {
        "-- REGION --"
    }
} else {
    match state.mode {
        Mode::Normal => "-- NORMAL --",
        // ... etc
    }
};
```

---

## 6. Vim モードとの相互作用

| 動作 | Emacs での影響 | Vim での影響 |
|------|---------------|-------------|
| `mark` フィールドが `Some` | リージョン操作可能 | 無視（Vim では使用しない） |
| Vim から Emacs に切り替え | `mark` は None で初期化 | — |
| Emacs から Vim に切り替え | — | `mark` はクリア |
| ファイル読み込み | `mark` は None にリセット | 影響なし |

現時点ではキーマップの実行時切り替えは未実装なので、この interaction は将来的な考慮事項。

---

## 7. 実装フェーズ分割

### Phase 5a: Core（最小限）
**対象**: データ構造 + `C-space` + `C-w` kill-region

| ファイル | 変更内容 |
|----------|---------|
| `src/types.rs` | `EditorState` に `mark`, `region_active` 追加 |
| `src/editor.rs` | 新規 `fn set_mark()`, `fn kill_region()`, `fn has_active_region()`, `fn deactivate_region()` |
| `src/editor.rs` | `KillRing` 構造体追加 |
| `src/keymap/emacs.rs` | `C-space` で `set_mark()`, `C-w` を条件分岐に変更 |
| `src/renderer.rs` | リージョンのハイライト表示、ステータス表示 |

### Phase 5b: Copy + Yank
**対象**: `M-w` copy + `C-y` yank from kill-ring

| ファイル | 変更内容 |
|----------|---------|
| `src/editor.rs` | `fn copy_region()`, `fn yank_from_kill_ring()` |
| `src/keymap/emacs.rs` | `M-w` 追加、`C-y` を kill-ring yank に変更 |
| `src/editor.rs` | 古い `yank_pop()` (undo) を削除 |

### Phase 5c: Yank-pop + C-g deactivate
**対象**: `M-y` + `C-g` region-aware

| ファイル | 変更内容 |
|----------|---------|
| `src/editor.rs` | `fn yank_pop_from_kill_ring()`（yank 履歴トラッキング） |
| `src/keymap/emacs.rs` | `M-y` 追加、`C-g` に `deactivate_region()` 追加 |

---

## 8. エッジケース

### 8.1 マークだけあってリージョンがない
```
C-space (mark at col 5)
C-space (mark at col 5, still)
```
→ `region_active = true`、`mark == cursor` → リージョンなし。`C-w` は kill-word を実行。

### 8.2 リージョンが空文字列
```
C-space (mark at line 1 col 5)
→ move to line 1 col 5 (no movement)
```
→ `has_active_region()` が false を返す → `C-w` は kill-word。

### 8.3 複数行リージョン
```
C-space (mark at line 1 col 3)
→ move to line 3 col 5
C-w
```
→ `normalize_selection()` が正しく範囲を計算。`kill_region()` が全行を削除し、kill-ring に格納。

### 8.4 リージョン逆方向
```
C-space (mark at line 5 col 10)
→ move to line 2 col 3
C-w
```
→ `normalize_selection()` が開始/終了を正規化するので、方向に関わらず正しく動作。

### 8.5 マークの暗黙的クリア
リージョン操作 (`C-w`, `M-w`) 後は自動で `deactivate_region()` を呼ぶ（transient-mark-mode 的な動作）。

---

## 9. テスト計画

```rust
// types.rs
#[test]
fn mark_defaults_to_none() { ... }

// editor.rs
#[test]
fn set_mark_records_cursor_position() { ... }
#[test]
fn has_active_region_false_when_mark_equals_cursor() { ... }
#[test]
fn has_active_region_true_when_mark_differs() { ... }
#[test]
fn kill_region_deletes_selected_text() { ... }
#[test]
fn kill_region_pushes_to_kill_ring() { ... }
#[test]
fn kill_region_pushes_to_register() { ... }
#[test]
fn copy_region_copies_without_deleting() { ... }
#[test]
fn deactivate_region_sets_region_active_false() { ... }
#[test]
fn kill_region_with_reverse_selection() { ... }

// kill_ring.rs
#[test]
fn kill_ring_push_and_yank() { ... }
#[test]
fn kill_ring_cycles_entries() { ... }
#[test]
fn kill_ring_max_entries() { ... }
```
