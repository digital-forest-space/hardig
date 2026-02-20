use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, Wrap};
use solana_sdk::signature::Signer;

use crate::app::{self, App, FormKind, Screen};

pub fn draw(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // title bar
            Constraint::Min(10),   // main content
            Constraint::Length(3),  // action bar
            Constraint::Length(6), // message log
        ])
        .split(frame.area());

    draw_title_bar(frame, app, chunks[0]);

    match app.screen {
        Screen::Dashboard => draw_dashboard(frame, app, chunks[1]),
        Screen::Form => draw_form(frame, app, chunks[1]),
        Screen::Confirm => draw_confirm(frame, app, chunks[1]),
        Screen::Result => draw_result(frame, app, chunks[1]),
    }

    draw_action_bar(frame, app, chunks[2]);
    draw_message_log(frame, app, chunks[3]);
}

fn draw_title_bar(frame: &mut Frame, app: &App, area: Rect) {
    let wallet = app.keypair.pubkey().to_string();
    let short_wallet = format!("{}..{}", &wallet[..4], &wallet[wallet.len() - 4..]);
    let role_str = app
        .my_permissions
        .map(app::permissions_name)
        .unwrap_or_else(|| "No Position".into());
    let refresh_str = app
        .last_refresh
        .map(|t| format!("{}s ago", t.elapsed().as_secs()))
        .unwrap_or_else(|| "never".into());

    let mf_status = if app.mayflower_initialized {
        "CPI Ready"
    } else {
        ""
    };

    let title = if mf_status.is_empty() {
        format!(
            " Härdig | {} | Role: {} | Last refresh: {} ",
            short_wallet, role_str, refresh_str,
        )
    } else {
        format!(
            " Härdig | {} | Role: {} | Nirvana: {} | Last refresh: {} ",
            short_wallet, role_str, mf_status, refresh_str,
        )
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(Color::Cyan));
    frame.render_widget(block, area);
}

fn draw_dashboard(frame: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(10), // position panel
            Constraint::Min(5),    // keyring panel
        ])
        .split(area);

    draw_position_panel(frame, app, chunks[0]);
    draw_keyring_panel(frame, app, chunks[1]);
}

fn draw_position_panel(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Position ")
        .border_style(Style::default().fg(Color::Yellow));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if !app.protocol_exists {
        let text = Paragraph::new(
            "Protocol not initialized. Press [I] to initialize.",
        );
        frame.render_widget(text, inner);
        return;
    }

    let pos = match &app.position {
        Some(p) => p,
        None => {
            let text =
                Paragraph::new("No position found. Press [n] to create one, or [r] to refresh.");
            frame.render_widget(text, inner);
            return;
        }
    };

    let position_pda = app
        .position_pda
        .map(|p| app::short_pubkey(&p))
        .unwrap_or_default();
    let lines = vec![
        Line::from(vec![
            Span::styled("  Position: ", Style::default().fg(Color::Gray)),
            Span::raw(position_pda),
            Span::raw("    "),
            Span::styled("Admin Asset: ", Style::default().fg(Color::Gray)),
            Span::raw(app::short_pubkey(&pos.admin_asset)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Deposited: ", Style::default().fg(Color::Gray)),
            Span::styled(
                format!("{} {}", app::lamports_to_sol(pos.deposited_nav),
                    app.market_config.as_ref().map(|mc| app::nav_token_name(&mc.nav_mint)).unwrap_or("shares")),
                Style::default().fg(Color::Green),
            ),
            Span::raw("    "),
            Span::styled("Debt: ", Style::default().fg(Color::Gray)),
            Span::styled(
                format!("{} SOL", app::lamports_to_sol(pos.user_debt)),
                Style::default().fg(if pos.user_debt > 0 {
                    Color::Red
                } else {
                    Color::White
                }),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Borrow Capacity: ", Style::default().fg(Color::Gray)),
            Span::styled(
                format!("{} SOL", app::lamports_to_sol(app.mf_borrow_capacity)),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
    ];

    let para = Paragraph::new(Text::from(lines));
    frame.render_widget(para, inner);
}

fn draw_keyring_panel(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" Keyring ({} keys) ", app.keyring.len()))
        .border_style(Style::default().fg(Color::Magenta));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if app.keyring.is_empty() {
        let text = Paragraph::new("  No keys found.");
        frame.render_widget(text, inner);
        return;
    }

    let header = Row::new(vec!["", "Name", "Asset", "Held"])
        .style(Style::default().add_modifier(Modifier::BOLD))
        .bottom_margin(0);

    let is_admin = |k: &app::KeyEntry| k.permissions == hardig::state::PRESET_ADMIN;

    let mut rows: Vec<Row> = Vec::new();
    for (i, k) in app.keyring.iter().enumerate() {
        let marker = if i == app.key_cursor { ">" } else { " " };
        let held = if k.held_by_signer { "YOU" } else { "" };
        let display_name = if k.name.is_empty() {
            if is_admin(k) { "Admin".to_string() } else { "Delegated".to_string() }
        } else {
            k.name.clone()
        };
        let style = if k.held_by_signer {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        };
        rows.push(
            Row::new(vec![
                marker.to_string(),
                display_name,
                app::short_pubkey(&k.asset),
                held.to_string(),
            ])
            .style(style),
        );
        // Sub-rows for delegated keys: permissions, then rate limits
        if !is_admin(k) {
            let sub_style = Style::default().fg(Color::DarkGray);
            let sub_row = |text: String| {
                Row::new(vec![String::new(), format!("  {}", text), String::new(), String::new()])
                    .style(sub_style)
            };
            rows.push(sub_row(app::permissions_name(k.permissions)));
            if let Some(ref bucket) = k.sell_bucket {
                let nav = app.market_config.as_ref().map(|mc| app::nav_token_name(&mc.nav_mint)).unwrap_or("shares");
                rows.push(sub_row(format!(
                    "Sell: {} {} / {}",
                    hardig::instructions::format_sol_amount(bucket.capacity),
                    nav,
                    hardig::instructions::slots_to_duration(bucket.refill_period),
                )));
            }
            if let Some(ref bucket) = k.borrow_bucket {
                rows.push(sub_row(format!(
                    "Borrow: {} SOL / {}",
                    hardig::instructions::format_sol_amount(bucket.capacity),
                    hardig::instructions::slots_to_duration(bucket.refill_period),
                )));
            }
        }
    }

    let widths = [
        Constraint::Length(2),
        Constraint::Min(28),
        Constraint::Length(14),
        Constraint::Length(5),
    ];

    let table = Table::new(rows, widths).header(header);
    frame.render_widget(table, inner);
}

/// Read a form field value, using `input_buf` if that field is currently active.
fn live_field_value(app: &App, label: &str) -> Option<String> {
    for (i, (l, v)) in app.form_fields.iter().enumerate() {
        if l == label {
            return if i == app.input_field {
                Some(app.input_buf.clone())
            } else {
                Some(v.clone())
            };
        }
    }
    None
}

fn draw_form(frame: &mut Frame, app: &App, area: Rect) {
    let nav = app.market_config.as_ref()
        .map(|mc| app::nav_token_name(&mc.nav_mint))
        .unwrap_or("shares");
    let title = match app.form_kind {
        Some(FormKind::CreatePosition) => " Create Position ".to_string(),
        Some(FormKind::AuthorizeKey) => " Authorize Key ".to_string(),
        Some(FormKind::RevokeKey) => " Revoke Key ".to_string(),
        Some(FormKind::Buy) => format!(" Buy {} ", nav),
        Some(FormKind::Sell) => format!(" Sell {} ", nav),
        Some(FormKind::Borrow) => " Borrow ".to_string(),
        Some(FormKind::Repay) => " Repay ".to_string(),
        _ => " Form ".to_string(),
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();

    if let Some(info) = &app.form_info {
        lines.push(Line::from(Span::styled(
            format!("  {}", info),
            Style::default().fg(Color::Yellow),
        )));
        lines.push(Line::from(""));
    }

    for (i, (label, value)) in app.form_fields.iter().enumerate() {
        let is_active = i == app.input_field;
        let label_style = if is_active {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Gray)
        };

        // Special rendering for permissions checkboxes in AuthorizeKey form
        if matches!(app.form_kind, Some(FormKind::AuthorizeKey)) && i == 1 {
            lines.push(Line::from(Span::styled("  Permissions:", label_style)));
            let bits = app.perm_bits;
            let perms: [(u8, &str); 7] = [
                (hardig::state::PERM_BUY, "1 Buy"),
                (hardig::state::PERM_SELL, "2 Sell"),
                (hardig::state::PERM_BORROW, "3 Borrow"),
                (hardig::state::PERM_REPAY, "4 Repay"),
                (hardig::state::PERM_REINVEST, "5 Reinvest"),
                (hardig::state::PERM_LIMITED_SELL, "6 LtdSell"),
                (hardig::state::PERM_LIMITED_BORROW, "7 LtdBorrow"),
            ];
            // Render permissions on two rows: 0-4 on first, 5-6 on second
            for row_range in [0..5, 5..7] {
                let mut spans = vec![Span::raw("    ")];
                for idx in row_range {
                    let (perm, name) = perms[idx];
                    let checked = bits & perm != 0;
                    let focused = is_active && idx == app.perm_cursor;
                    let marker = if checked { "x" } else { " " };
                    let mut style = if checked {
                        Style::default().fg(Color::Green)
                    } else {
                        Style::default().fg(Color::White)
                    };
                    if focused {
                        style = style.add_modifier(Modifier::BOLD | Modifier::UNDERLINED);
                    }
                    spans.push(Span::styled(format!("[{}] {}  ", marker, name), style));
                }
                lines.push(Line::from(spans));
            }
            lines.push(Line::from(vec![
                Span::raw("    "),
                Span::styled(
                    format!("= {} (0x{:02X})", app::permissions_name(bits), bits),
                    Style::default().fg(Color::DarkGray),
                ),
            ]));
            lines.push(Line::from(""));
            continue;
        }

        let display_value = if is_active { &app.input_buf } else { value };
        let cursor = if is_active { "_" } else { "" };

        // Handle multiline values (for revoke key list)
        if value.contains('\n') && !is_active {
            lines.push(Line::from(Span::styled(
                format!("  {}:", label),
                label_style,
            )));
            for line in value.lines() {
                lines.push(Line::from(format!("    {}", line)));
            }
        } else {
            let spans = vec![
                Span::styled(format!("  {}: ", label), label_style),
                Span::raw(format!("{}{}", display_value, cursor)),
            ];
            lines.push(Line::from(spans));
        }

        // After a "Refill Minutes" field, show the computed total slots summary
        if label.contains("Refill Minutes") {
            let prefix = if label.starts_with("Sell") { "Sell" } else { "Borrow" };
            let days = live_field_value(app, &format!("{} Refill Days", prefix));
            let hours = live_field_value(app, &format!("{} Refill Hours", prefix));
            let mins = live_field_value(app, &format!("{} Refill Minutes", prefix));
            let slots = app::time_fields_to_slots(&days, &hours, &mins);
            if slots > 0 {
                lines.push(Line::from(vec![
                    Span::raw("    "),
                    Span::styled(
                        format!("= {} slots ({})", slots, app::slots_to_human(slots)),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]));
            }
        }

        lines.push(Line::from(""));
    }

    lines.push(Line::from(""));
    let on_perm_field =
        matches!(app.form_kind, Some(FormKind::AuthorizeKey)) && app.input_field == 1;
    let hints = if on_perm_field {
        "  [\u{2190}\u{2192}] Navigate  [Space/1-7] Toggle  [Enter] Submit  [Tab] Next  [Esc] Cancel"
    } else {
        "  [Enter] Submit  [Tab/Shift+Tab] Navigate fields  [Esc] Cancel"
    };
    lines.push(Line::from(Span::styled(
        hints,
        Style::default().fg(Color::DarkGray),
    )));

    let para = Paragraph::new(Text::from(lines));
    frame.render_widget(para, inner);
}

fn draw_confirm(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Confirm Transaction ")
        .border_style(Style::default().fg(Color::Red));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines: Vec<Line> = vec![Line::from("")];

    if let Some(action) = &app.pending_action {
        for desc_line in &action.description {
            lines.push(Line::from(format!("  {}", desc_line)));
        }
        lines.push(Line::from(""));
        lines.push(Line::from(format!(
            "  Instructions: {}",
            action.instructions.len()
        )));
        let total_accounts: usize = action
            .instructions
            .iter()
            .map(|ix| ix.accounts.len())
            .sum();
        lines.push(Line::from(format!("  Total accounts: {}", total_accounts)));
        if !action.extra_signers.is_empty() {
            lines.push(Line::from(format!(
                "  Extra signers: {} (new mint keypair)",
                action.extra_signers.len()
            )));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  Press [Y] to confirm and send, [N] or [Esc] to cancel",
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    )));

    let para = Paragraph::new(Text::from(lines));
    frame.render_widget(para, inner);
}

fn draw_result(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Transaction Result ")
        .border_style(Style::default().fg(Color::Green));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let sig_display = app
        .last_tx_signature
        .as_deref()
        .map(|s| {
            if s.len() > 44 {
                format!("{}...{}", &s[..20], &s[s.len() - 20..])
            } else {
                s.to_string()
            }
        })
        .unwrap_or_default();

    match (&app.pre_tx_snapshot, &app.position) {
        (Some(before), Some(pos)) => {
            let nav_name = app.market_config.as_ref()
                .map(|mc| app::nav_token_name(&mc.nav_mint))
                .unwrap_or("shares");
            let rows_data: Vec<(&str, u64, u64, &str)> = vec![
                ("Deposited", before.deposited_nav, pos.deposited_nav, nav_name),
                ("Debt", before.user_debt, pos.user_debt, "SOL"),
                ("Borrow Cap", before.borrow_capacity, app.mf_borrow_capacity, "SOL"),
            ];

            let rows: Vec<Row> = rows_data
                .iter()
                .map(|(label, bv, av, unit)| {
                    let delta = app::format_delta(*bv, *av);
                    let delta_color = if *av > *bv {
                        Color::Green
                    } else if *av < *bv {
                        Color::Red
                    } else {
                        Color::DarkGray
                    };
                    Row::new(vec![
                        Cell::from(*label).style(Style::default().fg(Color::Gray)),
                        Cell::from(format!("{} {}", app::lamports_to_sol(*bv), unit)),
                        Cell::from(format!("{} {}", app::lamports_to_sol(*av), unit)),
                        Cell::from(delta).style(Style::default().fg(delta_color)),
                    ])
                })
                .collect();

            let header = Row::new(vec!["", "Before", "After", "Delta"])
                .style(
                    Style::default()
                        .add_modifier(Modifier::BOLD)
                        .fg(Color::Cyan),
                )
                .bottom_margin(1);

            let widths = [
                Constraint::Length(14),
                Constraint::Length(18),
                Constraint::Length(18),
                Constraint::Length(18),
            ];

            let table_area = Rect {
                x: inner.x,
                y: inner.y + 3,
                width: inner.width,
                height: inner.height.saturating_sub(3),
            };

            let sig_line = Paragraph::new(vec![
                Line::from(""),
                Line::from(vec![
                    Span::styled("  Signature: ", Style::default().fg(Color::Gray)),
                    Span::styled(sig_display, Style::default().fg(Color::Green)),
                ]),
            ]);
            frame.render_widget(sig_line, inner);

            let table = Table::new(rows, widths).header(header);
            frame.render_widget(table, table_area);
        }
        _ => {
            let text = Paragraph::new(vec![
                Line::from(""),
                Line::from(vec![
                    Span::styled("  Signature: ", Style::default().fg(Color::Gray)),
                    Span::styled(sig_display, Style::default().fg(Color::Green)),
                ]),
                Line::from(""),
                Line::from("  Transaction completed successfully."),
            ]);
            frame.render_widget(text, inner);
        }
    }
}

fn draw_action_bar(frame: &mut Frame, app: &App, area: Rect) {
    let actions: String = match app.screen {
        Screen::Form => "[Enter] Submit  [Tab] Next  [Esc] Cancel".into(),
        Screen::Confirm => "[Y] Confirm  [N] Cancel".into(),
        Screen::Result => "[any key] Continue".into(),
        Screen::Dashboard => {
            if !app.protocol_exists {
                "[I] Init Protocol  [r] Refresh  [q] Quit".into()
            } else if app.position_pda.is_none() {
                "[n] New Position  [r] Refresh  [q] Quit".into()
            } else {
                let mut parts: Vec<&str> = Vec::new();

                // Financial actions (require CPI)
                if app.can_buy() { parts.push("[b]uy"); }
                if app.can_sell() { parts.push("[s]ell"); }
                if app.can_borrow() { parts.push("[d]borrow"); }
                if app.can_repay() { parts.push("[p]repay"); }
                if app.can_reinvest() { parts.push("[i]reinvest"); }

                // Admin key management (always available for admin)
                if app.has_perm(hardig::state::PERM_MANAGE_KEYS) {
                    parts.push("[a]uth");
                    parts.push("[x]revoke");
                }

                parts.push("[r]efresh");
                parts.push("[q]uit");
                parts.join(" ")
            }
        }
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Actions ")
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let para = Paragraph::new(Span::styled(
        format!(" {}", actions),
        Style::default().fg(Color::White),
    ));
    frame.render_widget(para, inner);
}

fn draw_message_log(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Log ");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines: Vec<Line> = app
        .message_log
        .iter()
        .map(|m| Line::from(format!(" > {}", m)))
        .collect();

    // Count how many visual rows the wrapped text will occupy.
    let width = inner.width as usize;
    let total_rows: usize = lines
        .iter()
        .map(|line| {
            let len = line.width();
            if width == 0 { 1 } else { 1_usize.max(len.div_ceil(width)) }
        })
        .sum();

    let visible = inner.height as u16;
    let scroll = (total_rows as u16).saturating_sub(visible);

    let para = Paragraph::new(Text::from(lines))
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0));
    frame.render_widget(para, inner);
}
