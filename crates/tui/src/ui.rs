use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, Wrap};
use solana_sdk::signature::Signer;

use crate::app::{self, App, FormKind, Screen};

fn format_duration(secs: i64) -> String {
    if secs < 0 { return "0s".into(); }
    let days = secs / 86400;
    let hours = (secs % 86400) / 3600;
    let mins = (secs % 3600) / 60;
    if days > 0 {
        if hours > 0 { format!("{}d {}h", days, hours) } else { format!("{}d", days) }
    } else if hours > 0 {
        if mins > 0 { format!("{}h {}m", hours, mins) } else { format!("{}h", hours) }
    } else if mins > 0 {
        format!("{}m", mins)
    } else {
        format!("{}s", secs)
    }
}

pub fn draw(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // title bar
            Constraint::Min(10),   // main content
            Constraint::Length(4),  // action bar (2 rows)
            Constraint::Length(6), // message log
        ])
        .split(frame.area());

    draw_title_bar(frame, app, chunks[0]);

    match app.screen {
        Screen::PositionList => draw_position_list(frame, app, chunks[1]),
        Screen::Dashboard => draw_dashboard(frame, app, chunks[1]),
        Screen::MarketPicker => draw_market_picker(frame, app, chunks[1]),
        Screen::PromoList => draw_promo_list(frame, app, chunks[1]),
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

    let pos_count = app.discovered_positions.len();
    let pos_indicator = if pos_count > 1 {
        // Find which position is active (by matching position_pda)
        let active_idx = app.position_pda
            .and_then(|pda| app.discovered_positions.iter().position(|dp| dp.position_pda == pda))
            .map(|i| i + 1)
            .unwrap_or(0);
        format!(" | Pos {}/{}", active_idx, pos_count)
    } else {
        String::new()
    };

    let title = if mf_status.is_empty() {
        format!(
            " Härdig | {} | Role: {}{} | Last refresh: {} ",
            short_wallet, role_str, pos_indicator, refresh_str,
        )
    } else {
        format!(
            " Härdig | {} | Role: {}{} | Nirvana: {} | Last refresh: {} ",
            short_wallet, role_str, pos_indicator, mf_status, refresh_str,
        )
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(Color::Cyan));
    frame.render_widget(block, area);
}

fn draw_position_list(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" Positions ({}) ", app.discovered_positions.len()))
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if app.discovered_positions.is_empty() {
        let text = Paragraph::new("  No positions found. Press [n] to create one.");
        frame.render_widget(text, inner);
        return;
    }

    let header = Row::new(vec!["", "Name", "Role", "Deposited", "Debt"])
        .style(Style::default().add_modifier(Modifier::BOLD))
        .bottom_margin(0);

    let mut rows: Vec<Row> = Vec::new();
    for (i, dp) in app.discovered_positions.iter().enumerate() {
        let marker = if i == app.position_list_cursor { ">" } else { " " };
        let role = if dp.is_admin {
            "Admin".to_string()
        } else if dp.is_recovery {
            "Recovery".to_string()
        } else {
            app::permissions_name(dp.permissions)
        };
        let display_name = if dp.name.is_empty() {
            app::short_pubkey(&dp.admin_asset)
        } else {
            dp.name.clone()
        };
        let style = if i == app.position_list_cursor {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        rows.push(
            Row::new(vec![
                marker.to_string(),
                display_name,
                role,
                format!("{} SOL", app::lamports_to_sol(dp.deposited_nav)),
                format!("{} SOL", app::lamports_to_sol(dp.user_debt)),
            ])
            .style(style)
        );
        // Sub-row: full admin asset address
        let sub_style = if i == app.position_list_cursor {
            Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        rows.push(
            Row::new(vec![
                String::new(),
                format!("  {}", dp.admin_asset),
                String::new(),
                String::new(),
                String::new(),
            ])
            .style(sub_style)
        );
    }

    let widths = [
        Constraint::Length(2),
        Constraint::Min(28),
        Constraint::Length(12),
        Constraint::Length(16),
        Constraint::Length(16),
    ];

    let table = Table::new(rows, widths).header(header);
    frame.render_widget(table, inner);
}

fn draw_market_picker(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" Select Market ({}) ", app.loaded_markets.len()))
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if app.loaded_markets.is_empty() {
        let text = Paragraph::new("  No markets loaded.");
        frame.render_widget(text, inner);
        return;
    }

    let header = Row::new(vec!["", "Market", "Floor Price", "Status"])
        .style(Style::default().add_modifier(Modifier::BOLD))
        .bottom_margin(0);

    let rows: Vec<Row> = app
        .loaded_markets
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let marker = if i == app.market_picker_cursor { ">" } else { " " };
            let status = if m.supported { "Supported" } else { "New" };
            let style = if i == app.market_picker_cursor {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            Row::new(vec![
                marker.to_string(),
                m.nav_symbol.clone(),
                format!("{:.6}", m.floor_price),
                status.to_string(),
            ])
            .style(style)
        })
        .collect();

    let widths = [
        Constraint::Length(2),
        Constraint::Min(16),
        Constraint::Length(14),
        Constraint::Length(12),
    ];

    let table = Table::new(rows, widths).header(header);
    frame.render_widget(table, inner);
}

fn draw_promo_list(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" Promos ({}) ", app.promos.len()))
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if app.promos.is_empty() {
        let text = Paragraph::new("  No promos found. Press [n] to create one.");
        frame.render_widget(text, inner);
        return;
    }

    let header = Row::new(vec!["", "Name", "Status", "Claims", "Permissions"])
        .style(Style::default().add_modifier(Modifier::BOLD))
        .bottom_margin(0);

    let mut rows: Vec<Row> = Vec::new();
    for (i, entry) in app.promos.iter().enumerate() {
        let marker = if i == app.promo_cursor { ">" } else { " " };
        let status = if entry.config.active { "Active" } else { "Paused" };
        let claims = if entry.config.max_claims == 0 {
            format!("{} / unlimited", entry.config.claims_count)
        } else {
            format!("{} / {}", entry.config.claims_count, entry.config.max_claims)
        };
        let perms = app::permissions_name(entry.config.permissions);
        let style = if i == app.promo_cursor {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let status_style = if entry.config.active {
            Style::default().fg(Color::Green)
        } else {
            Style::default().fg(Color::Red)
        };
        rows.push(
            Row::new(vec![
                Cell::from(marker.to_string()).style(style),
                Cell::from(entry.config.name_suffix.clone()).style(style),
                Cell::from(status.to_string()).style(status_style),
                Cell::from(claims).style(style),
                Cell::from(perms).style(style),
            ])
        );
        // Sub-row: borrow/sell capacity summary
        let sub_style = if i == app.promo_cursor {
            Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let mut detail_parts: Vec<String> = Vec::new();
        if entry.config.borrow_capacity > 0 {
            detail_parts.push(format!(
                "Borrow: {} SOL / {}",
                app::lamports_to_sol(entry.config.borrow_capacity),
                app::slots_to_human(entry.config.borrow_refill_period),
            ));
        }
        if entry.config.sell_capacity > 0 {
            detail_parts.push(format!(
                "Sell: {} SOL / {}",
                app::lamports_to_sol(entry.config.sell_capacity),
                app::slots_to_human(entry.config.sell_refill_period),
            ));
        }
        if entry.config.min_deposit_lamports > 0 {
            detail_parts.push(format!(
                "Min deposit: {} SOL",
                app::lamports_to_sol(entry.config.min_deposit_lamports),
            ));
        }
        if !detail_parts.is_empty() {
            rows.push(
                Row::new(vec![
                    Cell::from(String::new()),
                    Cell::from(format!("  {}", detail_parts.join("  |  "))),
                    Cell::from(String::new()),
                    Cell::from(String::new()),
                    Cell::from(String::new()),
                ])
                .style(sub_style),
            );
        }
    }

    let widths = [
        Constraint::Length(2),
        Constraint::Min(20),
        Constraint::Length(8),
        Constraint::Length(16),
        Constraint::Min(20),
    ];

    let table = Table::new(rows, widths).header(header);
    frame.render_widget(table, inner);
}

fn draw_dashboard(frame: &mut Frame, app: &App, area: Rect) {
    let has_recovery = app.position.as_ref()
        .map(|p| p.recovery_asset != solana_sdk::pubkey::Pubkey::default())
        .unwrap_or(false);
    let has_activity = app.position.as_ref()
        .map(|p| p.last_admin_activity > 0)
        .unwrap_or(false);
    let panel_height = 10
        + if has_activity { 2 } else { 0 }
        + if has_recovery { 2 } else { 0 };
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(panel_height), // position panel
            Constraint::Min(5),              // keyring panel
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
    let admin_name = app
        .discovered_positions
        .get(app.position_list_cursor)
        .map(|dp| dp.name.as_str())
        .unwrap_or("");
    let mut top_spans = vec![
        Span::styled("  Position: ", Style::default().fg(Color::Gray)),
        Span::raw(position_pda),
        Span::raw("    "),
        Span::styled("Admin Asset: ", Style::default().fg(Color::Gray)),
        Span::raw(app::short_pubkey(&pos.current_admin_asset)),
    ];
    if !admin_name.is_empty() {
        top_spans.push(Span::raw("    "));
        top_spans.push(Span::styled("Name: ", Style::default().fg(Color::Gray)));
        top_spans.push(Span::styled(
            admin_name.to_string(),
            Style::default().fg(Color::Cyan),
        ));
    }
    let mut lines = vec![
        Line::from(top_spans),
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

    // Last admin activity + recovery status
    if pos.last_admin_activity > 0 {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let ago = now - pos.last_admin_activity;
        let ago_str = format_duration(ago);

        let has_recovery = pos.recovery_asset != solana_sdk::pubkey::Pubkey::default();
        let mut spans = vec![
            Span::styled("  Last Activity: ", Style::default().fg(Color::Gray)),
            Span::styled(format!("{} ago", ago_str), Style::default().fg(Color::White)),
        ];

        if has_recovery {
            let remaining = pos.recovery_lockout_secs - ago;
            if remaining > 0 {
                spans.push(Span::raw("    "));
                spans.push(Span::styled("Recoverable in: ", Style::default().fg(Color::Gray)));
                spans.push(Span::styled(
                    format_duration(remaining),
                    Style::default().fg(Color::Yellow),
                ));
            } else {
                spans.push(Span::raw("    "));
                spans.push(Span::styled(
                    "RECOVERABLE NOW",
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                ));
            }
        }
        lines.push(Line::from(spans));
    }

    // Recovery config line
    if pos.recovery_asset != solana_sdk::pubkey::Pubkey::default() {
        let grace_str = format_duration(pos.recovery_lockout_secs);
        let locked_str = if pos.recovery_config_locked { " (locked)" } else { "" };
        lines.push(Line::from(vec![
            Span::styled("  Recovery: ", Style::default().fg(Color::Gray)),
            Span::styled(
                format!("{} grace{}", grace_str, locked_str),
                Style::default().fg(Color::Green),
            ),
            Span::raw("    "),
            Span::styled("Key: ", Style::default().fg(Color::Gray)),
            Span::raw(app::short_pubkey(&pos.recovery_asset)),
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

    let header = Row::new(vec!["", "Name", "Asset", "Held"])
        .style(Style::default().add_modifier(Modifier::BOLD))
        .bottom_margin(0);

    let is_admin = |k: &app::KeyEntry| k.permissions == hardig::state::PRESET_ADMIN;
    let is_recovery = |k: &app::KeyEntry| {
        app.position.as_ref().map_or(false, |pos| {
            pos.recovery_asset != solana_sdk::pubkey::Pubkey::default()
                && k.asset == pos.recovery_asset
        })
    };

    let mut rows: Vec<Row> = Vec::new();
    for (i, k) in app.keyring.iter().enumerate() {
        let marker = if i == app.key_cursor { ">" } else { " " };
        let held = if k.held_by_signer { "YOU" } else { "" };
        let recovery = is_recovery(k);
        let display_name = if !k.name.is_empty() {
            k.name.clone()
        } else if is_admin(k) {
            "Admin".to_string()
        } else if recovery {
            "Recovery".to_string()
        } else {
            "Delegated".to_string()
        };
        let style = if k.held_by_signer {
            Style::default().fg(Color::Yellow)
        } else if recovery {
            Style::default().fg(Color::Green)
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
        // Sub-rows for recovery keys: show grace period info
        if recovery {
            let sub_style = Style::default().fg(Color::DarkGray);
            let sub_row = |text: String| {
                Row::new(vec![String::new(), format!("  {}", text), String::new(), String::new()])
                    .style(sub_style)
            };
            if let Some(ref pos) = app.position {
                let grace = format_duration(pos.recovery_lockout_secs);
                let locked = if pos.recovery_config_locked { " (locked)" } else { "" };
                rows.push(sub_row(format!("Recovery \u{2014} {}{}",  grace, locked)));
            }
        }
        // Sub-rows for delegated keys: permissions, then rate limits
        else if !is_admin(k) {
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
        Some(FormKind::ConfigureRecovery) => " Configure Recovery ".to_string(),
        Some(FormKind::CreatePromo) => " Create Promo ".to_string(),
        Some(FormKind::UpdatePromo) => " View Promo ".to_string(),
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
        for info_line in info.lines() {
            lines.push(Line::from(Span::styled(
                format!("  {}", info_line),
                Style::default().fg(Color::Yellow),
            )));
        }
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

        // Inline section hints for ConfigureRecovery form
        if matches!(app.form_kind, Some(FormKind::ConfigureRecovery)) {
            let hint = match i {
                0 => Some("Wallet that receives the recovery key NFT"),
                1 => Some("How long admin must be idle before recovery"),
                4 => Some("If true, settings become permanent"),
                5 => Some("Optional name suffix for the recovery NFT"),
                _ => None,
            };
            if let Some(h) = hint {
                lines.push(Line::from(Span::styled(
                    format!("  {}", h),
                    Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC),
                )));
            }
        }

        // Special rendering for permissions checkboxes in AuthorizeKey and CreatePromo forms
        if (matches!(app.form_kind, Some(FormKind::AuthorizeKey)) || matches!(app.form_kind, Some(FormKind::CreatePromo))) && i == 1 {
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

        let display_value = if !app.form_readonly && is_active { &app.input_buf } else { value };
        let cursor = if !app.form_readonly && is_active { "_" } else { "" };

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
            let value_style = if app.form_readonly {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default()
            };
            let spans = vec![
                Span::styled(format!("  {}: ", label), label_style),
                Span::styled(format!("{}{}", display_value, cursor), value_style),
            ];
            lines.push(Line::from(spans));
        }

        // After "Grace Period Minutes" field, show computed total grace period
        if label == "Grace Period Minutes" {
            let days_s = live_field_value(app, "Grace Period Days");
            let hours_s = live_field_value(app, "Grace Period Hours");
            let mins_s = live_field_value(app, "Grace Period Minutes");
            let days: u64 = days_s.as_deref().unwrap_or("0").trim().parse().unwrap_or(0);
            let hours: u64 = hours_s.as_deref().unwrap_or("0").trim().parse().unwrap_or(0);
            let mins: u64 = mins_s.as_deref().unwrap_or("0").trim().parse().unwrap_or(0);
            let total_secs = (days * 86400 + hours * 3600 + mins * 60) as i64;
            if total_secs > 0 {
                lines.push(Line::from(vec![
                    Span::raw("    "),
                    Span::styled(
                        format!("= {}", format_duration(total_secs)),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]));
            }
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
        (matches!(app.form_kind, Some(FormKind::AuthorizeKey)) || matches!(app.form_kind, Some(FormKind::CreatePromo)))
        && app.input_field == 1;
    let hints = if app.form_locked {
        "  [Esc] Back"
    } else if app.form_readonly && matches!(app.form_kind, Some(FormKind::UpdatePromo)) {
        "  [Enter] Toggle active/paused  [m] Change max claims  [Esc] Back"
    } else if app.form_readonly {
        "  [Enter] Edit  [Esc] Back"
    } else if on_perm_field {
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
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Actions ")
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines: Vec<Line> = match app.screen {
        Screen::PositionList => vec![
            Line::from(vec![
                action_key("[Enter]"), action_label("Open  "),
                action_key("[n]"), action_label("New  "),
                action_key("[r]"), action_label("Refresh  "),
                action_key("[q]"), action_label("Quit"),
            ]),
        ],
        Screen::MarketPicker => vec![
            Line::from(vec![
                action_key("[Enter]"), action_label("Select  "),
                action_key("[Esc]"), action_label("Cancel  "),
                action_key("[q]"), action_label("Quit"),
            ]),
        ],
        Screen::PromoList => vec![
            Line::from(vec![
                action_key("[Enter]"), action_label("View  "),
                action_key("[n]"), action_label("New  "),
                action_key("[r]"), action_label("Refresh  "),
                action_key("[Esc]"), action_label("Back  "),
                action_key("[q]"), action_label("Quit"),
            ]),
        ],
        Screen::Form => vec![
            Line::from(vec![
                action_key("[Enter]"), action_label("Submit  "),
                action_key("[Tab]"), action_label("Next  "),
                action_key("[Esc]"), action_label("Cancel"),
            ]),
        ],
        Screen::Confirm => vec![
            Line::from(vec![
                action_key("[Y]"), action_label("Confirm  "),
                action_key("[N]"), action_label("Cancel"),
            ]),
        ],
        Screen::Result => vec![
            Line::from(vec![action_label("Press any key to continue")]),
        ],
        Screen::Dashboard => {
            if !app.protocol_exists {
                vec![Line::from(vec![
                    action_key("[I]"), action_label("Init  "),
                    action_key("[r]"), action_label("Refresh  "),
                    action_key("[q]"), action_label("Quit"),
                ])]
            } else if app.position_pda.is_none() {
                vec![Line::from(vec![
                    action_key("[n]"), action_label("New  "),
                    action_key("[r]"), action_label("Refresh  "),
                    action_key("[q]"), action_label("Quit"),
                ])]
            } else {
                // Row 1: Financial actions
                let mut row1 = Vec::new();
                if app.can_buy() { row1.extend([action_key("[b]"), action_label("uy  ")]); }
                if app.can_sell() { row1.extend([action_key("[s]"), action_label("ell  ")]); }
                if app.can_borrow() { row1.extend([action_key("[d]"), action_label("borrow  ")]); }
                if app.can_repay() { row1.extend([action_key("[p]"), action_label("repay  ")]); }
                if app.can_reinvest() { row1.extend([action_key("[i]"), action_label("reinvest  ")]); }
                if row1.is_empty() {
                    row1.push(Span::styled(" No actions available", Style::default().fg(Color::DarkGray)));
                }

                // Row 2: Admin + navigation
                let mut row2 = Vec::new();
                if app.has_perm(hardig::state::PERM_MANAGE_KEYS) {
                    row2.extend([action_key("[a]"), action_label("uth  ")]);
                    row2.extend([action_key("[x]"), action_label("revoke  ")]);
                    row2.extend([action_key("[h]"), action_label("beat  ")]);
                    row2.extend([action_key("[c]"), action_label("recovery  ")]);
                    row2.extend([action_key("[P]"), action_label("romo  ")]);
                }
                // Execute recovery is available to anyone holding a recovery key
                if app.position.as_ref().map(|p| p.recovery_asset != solana_sdk::pubkey::Pubkey::default()).unwrap_or(false) {
                    row2.extend([action_key("[e]"), action_label("recover  ")]);
                }
                row2.extend([action_key("[n]"), action_label("ew  ")]);
                row2.extend([action_key("[r]"), action_label("efresh  ")]);
                row2.extend([action_key("[q]"), action_label("uit")]);
                if app.discovered_positions.len() > 1 {
                    row2.extend([Span::raw("  "), action_key("[Esc]"), action_label("Back")]);
                }

                vec![Line::from(row1), Line::from(row2)]
            }
        }
    };

    let para = Paragraph::new(Text::from(lines));
    frame.render_widget(para, inner);
}

fn action_key(key: &str) -> Span<'_> {
    Span::styled(key, Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
}

fn action_label(label: &str) -> Span<'_> {
    Span::styled(label, Style::default().fg(Color::White))
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
