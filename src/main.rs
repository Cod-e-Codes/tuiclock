use chrono::{Local, Timelike};
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
};
use std::{env, io, io::stdout, time::Duration};

#[derive(Clone, Copy)]
enum CellType {
    Empty,
    Circle,
    Numeral,
    HourHand,
    MinuteHand,
    SecondHand,
}

/// Bresenham line algorithm to draw a line from (x0, y0) to (x1, y1)
fn draw_line(
    grid: &mut [Vec<(char, CellType)>],
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    c: char,
    cell_type: CellType,
) {
    let dx = (x1 - x0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy = -(y1 - y0).abs();
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    let mut x = x0;
    let mut y = y0;

    loop {
        if y >= 0 && y < grid.len() as i32 && x >= 0 && x < grid[0].len() as i32 {
            grid[y as usize][x as usize] = (c, cell_type);
        }
        if x == x1 && y == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
}

/// Generate the ASCII clock face
fn draw_clock(area: Rect, use_color: bool) -> Vec<Line<'static>> {
    let width = area.width as i32;
    let height = area.height as i32;
    let mut grid = vec![vec![(' ', CellType::Empty); width as usize]; height as usize];

    let cx = width / 2;
    let cy = height / 2;
    let radius = (width.min(height) / 2) - 2;

    // Fixed scaling for terminal character aspect ratio (typically ~0.5 height/width)
    let y_scale = 0.5;

    // Draw the clock circle with wider tolerance to avoid gaps
    for y in 0..height {
        for x in 0..width {
            let dx = x - cx;
            let dy = ((y - cy) as f64 / y_scale) as i32;
            let dist = ((dx * dx + dy * dy) as f64).sqrt();
            if (dist - radius as f64).abs() < 1.0 {
                grid[y as usize][x as usize] = ('o', CellType::Circle);
            }
        }
    }

    // Roman numerals for hours (0=12 at top)
    let romans = [
        "XII", "I", "II", "III", "IV", "V", "VI", "VII", "VIII", "IX", "X", "XI",
    ];

    // Only draw numerals if terminal is wide enough to avoid clipping
    let min_width_for_numerals = 60;
    if width >= min_width_for_numerals {
        for (i, numeral) in romans.iter().enumerate() {
            let angle = i as f64 * std::f64::consts::TAU / 12.0;
            let num_len = numeral.len() as i32;
            let rad_pos = radius as f64 * 0.88;

            let center_x = cx + (angle.sin() * rad_pos) as i32;
            let center_y = cy - (angle.cos() * rad_pos * y_scale) as i32;

            for (j, ch) in numeral.chars().enumerate() {
                let offset_x = j as i32 - (num_len - 1) / 2;
                let char_x = center_x + offset_x;
                let char_y = center_y;

                if char_y >= 0
                    && char_y < height
                    && char_x >= 0
                    && char_x < width
                    && grid[char_y as usize][char_x as usize].0 == ' '
                {
                    grid[char_y as usize][char_x as usize] = (ch, CellType::Numeral);
                }
            }
        }
    } else {
        // Fallback to ticks on narrow terminals
        for i in 0..12 {
            let angle = i as f64 * std::f64::consts::TAU / 12.0;
            let x = cx + (angle.sin() * (radius as f64 * 0.92)) as i32;
            let y = cy - (angle.cos() * (radius as f64 * 0.92) * y_scale) as i32;
            if y >= 0 && y < height && x >= 0 && x < width {
                grid[y as usize][x as usize] = ('|', CellType::Numeral);
            }
        }
    }

    let now = Local::now();
    let secs = now.second() as f64 + (now.timestamp_subsec_nanos() as f64 / 1_000_000_000.0);
    let mins = now.minute() as f64 + secs / 60.0;
    let hours = (now.hour() % 12) as f64 + mins / 60.0;

    let second_angle = (secs / 60.0) * std::f64::consts::TAU;
    let minute_angle = (mins / 60.0) * std::f64::consts::TAU;
    let hour_angle = (hours / 12.0) * std::f64::consts::TAU;

    // Scaled hand lengths
    let hour_length = (radius as f64 * 0.45) as i32;
    let minute_length = (radius as f64 * 0.75) as i32;
    let second_length = (radius as f64 * 0.9) as i32;

    // Hour hand
    let hx = cx + (hour_angle.sin() * hour_length as f64) as i32;
    let hy = cy - (hour_angle.cos() * hour_length as f64 * y_scale) as i32;
    draw_line(&mut grid, cx, cy, hx, hy, '#', CellType::HourHand);

    // Minute hand
    let mx = cx + (minute_angle.sin() * minute_length as f64) as i32;
    let my = cy - (minute_angle.cos() * minute_length as f64 * y_scale) as i32;
    draw_line(&mut grid, cx, cy, mx, my, '*', CellType::MinuteHand);

    // Second hand
    let sx = cx + (second_angle.sin() * second_length as f64) as i32;
    let sy = cy - (second_angle.cos() * second_length as f64 * y_scale) as i32;
    draw_line(&mut grid, cx, cy, sx, sy, '.', CellType::SecondHand);

    // Convert grid to styled lines
    grid.into_iter()
        .map(|row| {
            let spans: Vec<Span> = row
                .into_iter()
                .map(|(ch, cell_type)| {
                    if use_color {
                        let style = match cell_type {
                            CellType::Empty => Style::default(),
                            CellType::Circle => Style::default().fg(Color::Cyan),
                            CellType::Numeral => Style::default().fg(Color::Yellow),
                            CellType::HourHand => Style::default().fg(Color::Green),
                            CellType::MinuteHand => Style::default().fg(Color::Blue),
                            CellType::SecondHand => Style::default().fg(Color::Red),
                        };
                        Span::styled(ch.to_string(), style)
                    } else {
                        Span::raw(ch.to_string())
                    }
                })
                .collect();
            Line::from(spans)
        })
        .collect()
}

fn main() -> io::Result<()> {
    let use_color = env::args().any(|arg| arg == "--color");

    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    loop {
        terminal.draw(|f| {
            let size = f.area();
            let lines = draw_clock(size, use_color);
            f.render_widget(Paragraph::new(lines), size);
        })?;

        if event::poll(Duration::from_millis(16))?
            && matches!(
                event::read()?,
                Event::Key(KeyEvent {
                    code: KeyCode::Char('q'),
                    ..
                })
            )
        {
            break;
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    Ok(())
}
