//! Plain-text output helpers: aligned tables and wrapped lists.

/// A text table with a header row and aligned columns.
pub struct Table {
    headers: Vec<String>,
    right: Vec<bool>,
    rows: Vec<Vec<String>>,
}

impl Table {
    /// A table with these column headers (all left-aligned).
    pub fn new(headers: &[&str]) -> Self {
        Table {
            headers: headers.iter().map(|h| (*h).to_string()).collect(),
            right: vec![false; headers.len()],
            rows: Vec::new(),
        }
    }

    /// Right-aligns the given columns (for numbers).
    pub fn right_align(mut self, columns: &[usize]) -> Self {
        for &c in columns {
            if let Some(flag) = self.right.get_mut(c) {
                *flag = true;
            }
        }
        self
    }

    /// Adds a row. Missing cells are empty, extra cells are ignored.
    pub fn row(&mut self, cells: Vec<String>) {
        self.rows.push(cells);
    }

    /// The table, every line prefixed with `indent` and ending in `\n`.
    pub fn render(&self, indent: &str) -> String {
        let columns = self.headers.len();
        let cell = |row: &[String], c: usize| row.get(c).cloned().unwrap_or_default();
        let mut widths: Vec<usize> = self.headers.iter().map(|h| h.chars().count()).collect();
        for row in &self.rows {
            for (c, width) in widths.iter_mut().enumerate() {
                *width = (*width).max(cell(row, c).chars().count());
            }
        }
        let line = |cells: Vec<String>| {
            let mut out = String::from(indent);
            for (c, text) in cells.iter().enumerate().take(columns) {
                if c > 0 {
                    out.push_str("  ");
                }
                let pad = widths[c].saturating_sub(text.chars().count());
                if self.right[c] {
                    out.push_str(&" ".repeat(pad));
                    out.push_str(text);
                } else {
                    out.push_str(text);
                    out.push_str(&" ".repeat(pad));
                }
            }
            let mut out = out.trim_end().to_string();
            out.push('\n');
            out
        };
        let mut out = line(self.headers.clone());
        for row in &self.rows {
            out.push_str(&line((0..columns).map(|c| cell(row, c)).collect()));
        }
        out
    }
}

/// Joins `items` with ", " and wraps the result at `width` characters; every
/// line starts with `indent`.
pub fn wrap_list(items: &[String], indent: &str, width: usize) -> String {
    let mut out = String::new();
    let mut line = String::from(indent);
    for (i, item) in items.iter().enumerate() {
        let piece = if i + 1 < items.len() {
            format!("{item},")
        } else {
            item.clone()
        };
        let fresh = line.len() == indent.len();
        if !fresh && line.chars().count() + 1 + piece.chars().count() > width {
            out.push_str(&line);
            out.push('\n');
            line = String::from(indent);
        }
        if line.len() > indent.len() {
            line.push(' ');
        }
        line.push_str(&piece);
    }
    if line.len() > indent.len() {
        out.push_str(&line);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aligns_columns() {
        let mut t = Table::new(&["Key", "Idx"]).right_align(&[1]);
        t.row(vec!["Esc".into(), "0".into()]);
        t.row(vec!["Backspace".into(), "36".into()]);
        assert_eq!(
            t.render("  "),
            "  Key        Idx\n  Esc          0\n  Backspace   36\n"
        );
    }

    #[test]
    fn wraps_lists() {
        let items: Vec<String> = ["Esc", "F1", "F2", "F3"].map(String::from).to_vec();
        assert_eq!(wrap_list(&items, "  ", 12), "  Esc, F1,\n  F2, F3\n");
        assert_eq!(wrap_list(&[], "  ", 12), "");
    }
}
