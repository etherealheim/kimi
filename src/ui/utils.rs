use ratatui::layout::{Constraint, Direction, Layout, Rect};

/// Creates a centered rectangle within the given area
///
/// # Arguments
/// * `percent_x` - Width as a percentage of the container (0-100)
/// * `percent_y` - Height as a percentage of the container (0-100)
/// * `r` - The container rectangle
///
/// # Returns
/// A centered rectangle with the specified dimensions
pub fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    // Clamp percentages to valid range
    let percent_x = percent_x.min(100);
    let percent_y = percent_y.min(100);

    // For very small terminals, use more of the available space
    let min_width = 30u16;
    let min_height = 10u16;

    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    let middle = popup_layout
        .get(1)
        .copied()
        .unwrap_or_else(|| popup_layout.first().copied().unwrap_or(r));

    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(middle);

    let mut result = horizontal
        .get(1)
        .copied()
        .unwrap_or_else(|| horizontal.first().copied().unwrap_or(r));

    // Ensure minimum dimensions for usability
    if result.width < min_width && r.width >= min_width {
        result.width = min_width.min(r.width);
        result.x = r.x + (r.width.saturating_sub(result.width)) / 2;
    }
    if result.height < min_height && r.height >= min_height {
        result.height = min_height.min(r.height);
        result.y = r.y + (r.height.saturating_sub(result.height)) / 2;
    }

    result
}
