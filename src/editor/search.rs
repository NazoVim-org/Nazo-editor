use crate::editor::Editor;
use crate::types::SearchDirection;

impl Editor {
    pub(crate) fn search_word_under_cursor(&mut self) {
        let (word, _, _) = self
            .buffer
            .get_word_range(self.state.cursor.line, self.state.cursor.col);
        if !word.is_empty() {
            let query = word.clone();
            self.do_search(&query, SearchDirection::Forward);
        }
    }

    pub(crate) fn search_next(&mut self) {
        if !self.search_query.is_empty() {
            let query = self.search_query.clone();
            let dir = self.search_direction;
            self.do_search(&query, dir);
        }
    }

    pub(crate) fn search_prev(&mut self) {
        if !self.search_query.is_empty() {
            let query = self.search_query.clone();
            let dir = match self.search_direction {
                SearchDirection::Forward => SearchDirection::Backward,
                SearchDirection::Backward => SearchDirection::Forward,
            };
            self.do_search(&query, dir);
        }
    }

    pub(crate) fn do_search(&mut self, query: &str, direction: SearchDirection) {
        self.search_query = query.to_string();
        self.search_direction = direction;
        self.search_results = self.buffer.search(query);

        if self.search_results.is_empty() {
            return;
        }

        if direction == SearchDirection::Forward {
            self.current_search_idx = self
                .search_results
                .iter()
                .position(|r| {
                    r.line > self.state.cursor.line
                        || (r.line == self.state.cursor.line && r.start_col > self.state.cursor.col)
                })
                .unwrap_or(0);
        } else {
            self.current_search_idx = self
                .search_results
                .iter()
                .rposition(|r| {
                    r.line < self.state.cursor.line
                        || (r.line == self.state.cursor.line && r.start_col < self.state.cursor.col)
                })
                .unwrap_or(self.search_results.len() - 1);
        }

        if let Some(result) = self.search_results.get(self.current_search_idx) {
            self.state.cursor.line = result.line;
            self.state.cursor.col = result.start_col;
        }
    }
}
