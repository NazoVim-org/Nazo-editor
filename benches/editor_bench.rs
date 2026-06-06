use criterion::{criterion_group, criterion_main, Criterion};
use ijevim::buffer::TextBuffer;
use ijevim::undo::UndoManager;

fn bench_buffer_insert(c: &mut Criterion) {
    c.bench_function("buffer_insert_char", |b| {
        b.iter(|| {
            let mut buf = TextBuffer::new();
            for i in 0..1000 {
                let line = (i / 80) + 1;
                let col = i % 80;
                buf.insert_char(line, col, 'a');
            }
            buf
        });
    });
}

fn bench_buffer_search(c: &mut Criterion) {
    let mut buf = TextBuffer::new();
    let content = (0..100)
        .map(|i| format!("line {} with some searchable content here\n", i))
        .collect::<String>();
    buf.insert(1, 0, &content);

    c.bench_function("buffer_search_100_lines", |b| {
        b.iter(|| buf.search("searchable"));
    });
}

fn bench_undo_redo(c: &mut Criterion) {
    c.bench_function("undo_100_edits", |b| {
        b.iter(|| {
            let mut buf = TextBuffer::new();
            let mut undo = UndoManager::new();
            let cursor = ijevim::types::Position { line: 1, col: 0 };

            for i in 0..100 {
                let line = (i / 80) + 1;
                let col = i % 80;
                let text = "x".to_string();
                buf.insert_char(line, col, 'x');
                undo.push(ijevim::undo::Edit {
                    edit_type: ijevim::undo::EditType::Insert { line, col, text },
                    cursor_before: cursor,
                    cursor_after: cursor,
                    modification_count: i,
                });
            }

            for _ in 0..100 {
                undo.undo(&mut buf);
            }
            buf
        });
    });
}

fn bench_line_operations(c: &mut Criterion) {
    let mut buf = TextBuffer::new();
    let content = (0..1000)
        .map(|i| format!("line {}\n", i))
        .collect::<String>();
    buf.insert(1, 0, &content);

    c.bench_function("get_line_1000_lines", |b| {
        b.iter(|| {
            for i in 1..=1000 {
                let _ = buf.get_line(i);
            }
        });
    });
}

criterion_group!(
    benches,
    bench_buffer_insert,
    bench_buffer_search,
    bench_undo_redo,
    bench_line_operations,
);
criterion_main!(benches);
