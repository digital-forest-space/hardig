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
        .my_role
        .map(app::role_name)
        .unwrap_or("No Position");
    let refresh_str = app
        .last_refresh
        .map(|t| format!("{}s ago", t.elapsed().as_secs()))
        .unwrap_or_else(|| "never".into());

    let mf_status = if app.mayflower_initialized {
        if app.atas_exist {
            "CPI Ready"
        } else {
            "Need ATAs"
        }
    } else if app.position_pda.is_some() {
        "Need Init"
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
            " Härdig | {} | Role: {} | Mayflower: {} | Last refresh: {} ",
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
            Constraint::Length(12), // position panel (increased for Mayflower info)
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
    let mut lines = vec![
        Line::from(vec![
            Span::styled("  Position: ", Style::default().fg(Color::Gray)),
            Span::raw(position_pda),
            Span::raw("    "),
            Span::styled("Admin Mint: ", Style::default().fg(Color::Gray)),
            Span::raw(app::short_pubkey(&pos.admin_nft_mint)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Deposited: ", Style::default().fg(Color::Gray)),
            Span::styled(
                format!("{} SOL", app::lamports_to_sol(pos.deposited_nav)),
                Style::default().fg(Color::Green),
            ),
            Span::raw("    "),
            Span::styled("User Debt: ", Style::default().fg(Color::Gray)),
            Span::styled(
                format!("{} SOL", app::lamports_to_sol(pos.user_debt)),
                Style::default().fg(if pos.user_debt > 0 {
                    Color::Red
                } else {
                    Color::White
                }),
            ),
            Span::raw("    "),
            Span::styled("Protocol Debt: ", Style::default().fg(Color::Gray)),
            Span::raw(format!("{} SOL", app::lamports_to_sol(pos.protocol_debt))),
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

    // Mayflower state
    if app.mayflower_initialized {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled(
                "  Mayflower: ",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("wSOL: ", Style::default().fg(Color::Gray)),
            Span::raw(format!("{} SOL", app::lamports_to_sol(app.wsol_balance))),
            Span::raw("    "),
            Span::styled("navSOL: ", Style::default().fg(Color::Gray)),
            Span::raw(format!(
                "{} SOL",
                app::lamports_to_sol(app.nav_sol_balance)
            )),
            Span::raw("    "),
            Span::styled("ATAs: ", Style::default().fg(Color::Gray)),
            Span::styled(
                if app.atas_exist { "OK" } else { "Missing" },
                Style::default().fg(if app.atas_exist {
                    Color::Green
                } else {
                    Color::Red
                }),
            ),
        ]));
    } else {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled(
                "  Mayflower: ",
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(
                "Not initialized",
                Style::default().fg(Color::DarkGray),
            ),
            if app.my_role == Some(app::KeyRole::Admin) {
                Span::styled(
                    " - Press [S] to setup",
                    Style::default().fg(Color::Yellow),
                )
            } else {
                Span::raw("")
            },
        ]));
    }

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

    let header = Row::new(vec!["", "Role", "Mint", "Held"])
        .style(Style::default().add_modifier(Modifier::BOLD))
        .bottom_margin(0);

    let rows: Vec<Row> = app
        .keyring
        .iter()
        .enumerate()
        .map(|(i, k)| {
            let marker = if i == app.key_cursor { ">" } else { " " };
            let held = if k.held_by_signer { "YOU" } else { "" };
            let style = if k.held_by_signer {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default()
            };
            Row::new(vec![
                marker.to_string(),
                app::role_name(k.role).to_string(),
                app::short_pubkey(&k.mint),
                held.to_string(),
            ])
            .style(style)
        })
        .collect();

    let widths = [
        Constraint::Length(2),
        Constraint::Length(12),
        Constraint::Length(14),
        Constraint::Length(5),
    ];

    let table = Table::new(rows, widths).header(header);
    frame.render_widget(table, inner);
}

fn draw_form(frame: &mut Frame, app: &App, area: Rect) {
    let title = match app.form_kind {
        Some(FormKind::CreatePosition) => " Create Position ",
        Some(FormKind::AuthorizeKey) => " Authorize Key ",
        Some(FormKind::RevokeKey) => " Revoke Key ",
        Some(FormKind::Buy) => " Buy navSOL ",
        Some(FormKind::Sell) => " Sell navSOL ",
        Some(FormKind::Borrow) => " Borrow ",
        Some(FormKind::Repay) => " Repay ",
        _ => " Form ",
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();

    for (i, (label, value)) in app.form_fields.iter().enumerate() {
        let is_active = i == app.input_field;
        let display_value = if is_active { &app.input_buf } else { value };

        let label_style = if is_active {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Gray)
        };

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
            lines.push(Line::from(vec![
                Span::styled(format!("  {}: ", label), label_style),
                Span::raw(format!("{}{}", display_value, cursor)),
            ]));
        }
        lines.push(Line::from(""));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  [Enter] Submit  [Tab] Next field  [Esc] Cancel",
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
            let rows_data: Vec<(&str, u64, u64)> = vec![
                ("Deposited", before.deposited_nav, pos.deposited_nav),
                ("User Debt", before.user_debt, pos.user_debt),
                ("Protocol Debt", before.protocol_debt, pos.protocol_debt),
                ("Borrow Cap", before.borrow_capacity, app.mf_borrow_capacity),
                ("wSOL", before.wsol_balance, app.wsol_balance),
                ("navSOL", before.nav_sol_balance, app.nav_sol_balance),
            ];

            let rows: Vec<Row> = rows_data
                .iter()
                .map(|(label, bv, av)| {
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
                        Cell::from(format!("{} SOL", app::lamports_to_sol(*bv))),
                        Cell::from(format!("{} SOL", app::lamports_to_sol(*av))),
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

                // One-time setup (admin only)
                if app.my_role == Some(app::KeyRole::Admin) && !app.cpi_ready() {
                    parts.push("[S]etup");
                }

                // Financial actions (require CPI)
                if app.can_buy() { parts.push("[b]uy"); }
                if app.can_sell() { parts.push("[s]ell"); }
                if app.can_borrow() { parts.push("[d]borrow"); }
                if app.can_repay() { parts.push("[p]repay"); }
                if app.can_reinvest() { parts.push("[i]reinvest"); }

                // Admin key management (always available for admin)
                if app.my_role == Some(app::KeyRole::Admin) {
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
