use syntect::{
    easy::HighlightLines, highlighting::ThemeSet, parsing::SyntaxSet, util::LinesWithEndings,
};

pub struct Highlighter {
    syntax_set: SyntaxSet,
    theme_set: ThemeSet,
}

impl Highlighter {
    pub fn new() -> Self {
        let syntax_set = SyntaxSet::load_defaults_newlines();
        let theme_set = ThemeSet::load_defaults();

        Self {
            syntax_set,
            theme_set,
        }
    }

    pub fn update(
        &self,
        text: &str,
        file_path: Option<&std::path::Path>,
    ) -> Vec<Vec<(syntect::highlighting::Style, String)>> {
        let theme = &self.theme_set.themes["base16-ocean.dark"];

        let syntax = file_path
            .and_then(|p| self.syntax_set.find_syntax_for_file(p).ok().flatten())
            .unwrap_or_else(|| self.syntax_set.find_syntax_plain_text());

        let mut highlighter = HighlightLines::new(syntax, theme);
        let mut result = Vec::new();

        for line in LinesWithEndings::from(text) {
            let ranges: Vec<(syntect::highlighting::Style, &str)> = highlighter
                .highlight_line(line, &self.syntax_set)
                .unwrap_or_default();
            let styled: Vec<(syntect::highlighting::Style, String)> = ranges
                .into_iter()
                .map(|(style, s)| (style, s.to_string()))
                .collect();
            result.push(styled);
        }

        result
    }
}

impl Default for Highlighter {
    fn default() -> Self {
        Self::new()
    }
}
