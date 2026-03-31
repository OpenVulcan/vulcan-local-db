use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Tabs, Wrap};

use crate::app::{ActivePage, App, ConfirmState, FieldKind, FormMode, FormState, Modal};
use crate::service::{ServiceState, ServiceStatus, format_exit_code};

const BG: Color = Color::Rgb(11, 16, 24);
const PANEL: Color = Color::Rgb(18, 26, 38);
const BORDER: Color = Color::Rgb(72, 94, 124);
const TEXT: Color = Color::Rgb(232, 236, 242);
const MUTED: Color = Color::Rgb(148, 162, 180);
const ACCENT: Color = Color::Rgb(255, 159, 67);
const SUCCESS: Color = Color::Rgb(46, 204, 113);
const WARN: Color = Color::Rgb(241, 196, 15);
const DANGER: Color = Color::Rgb(255, 107, 107);
const INFO: Color = Color::Rgb(88, 214, 141);
const EXTERNAL: Color = Color::Rgb(84, 160, 255);

pub fn render(frame: &mut Frame, app: &App) {
    frame.render_widget(
        Block::default().style(Style::default().bg(BG)),
        frame.area(),
    );

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(7),
            Constraint::Min(16),
            Constraint::Length(7),
        ])
        .split(frame.area());

    render_header(frame, app, layout[0]);
    match app.active_page {
        ActivePage::Installed => render_installed_page(frame, app, layout[1]),
        ActivePage::Workspace => render_workspace_page(frame, app, layout[1]),
        ActivePage::Output => render_output_page(frame, app, layout[1]),
    }
    render_footer(frame, app, layout[2]);

    if let Some(modal) = &app.modal {
        render_modal(frame, app, modal);
    }
}

fn render_header(frame: &mut Frame, app: &App, area: Rect) {
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(4), Constraint::Length(3)])
        .split(area);

    let installed_release = app
        .manager
        .state
        .release_tag
        .as_deref()
        .unwrap_or(app.text("未安装", "not installed"));
    let title = Paragraph::new(Text::from(vec![
        Line::from(vec![
            Span::styled(
                "VLDB Manager",
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(
                    "  {} | {} {} | release {}",
                    app.text("跨平台控制台", "Cross-platform control plane"),
                    app.text("语言", "lang"),
                    app.manager.state.language,
                    installed_release
                ),
                Style::default().fg(TEXT),
            ),
        ]),
        Line::from(Span::styled(
            format!(
                "{} {}   {} {}   {} {}",
                app.text("工作区", "workspace"),
                sanitize_display_text(&app.workspace.root.display().to_string()),
                app.text("安装目录", "install"),
                sanitize_display_text(&app.manager.paths.install_dir.display().to_string()),
                app.text("会话", "session"),
                app.launched_at
            ),
            Style::default().fg(MUTED),
        )),
    ]))
    .block(panel(app.text("概览", "Overview")))
    .wrap(Wrap { trim: true });
    frame.render_widget(title, sections[0]);

    let titles = ActivePage::all()
        .iter()
        .map(|page| {
            Line::from(Span::styled(
                app.page_label(*page),
                Style::default().fg(TEXT),
            ))
        })
        .collect::<Vec<_>>();
    let index = ActivePage::all()
        .iter()
        .position(|page| *page == app.active_page)
        .unwrap_or_default();
    let tabs = Tabs::new(titles)
        .block(panel(app.text("页面", "Pages")))
        .select(index)
        .style(Style::default().fg(MUTED))
        .highlight_style(Style::default().fg(ACCENT).add_modifier(Modifier::BOLD));
    frame.render_widget(tabs, sections[1]);
}

fn render_installed_page(frame: &mut Frame, app: &App, area: Rect) {
    let layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(40), Constraint::Min(60)])
        .split(area);
    render_instance_list(frame, app, layout[0]);

    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(9),
            Constraint::Length(9),
            Constraint::Min(8),
        ])
        .split(layout[1]);
    render_manager_summary(frame, app, right[0]);
    render_selected_instance(frame, app, right[1]);
    render_instance_config(frame, app, right[2]);
}

fn render_workspace_page(frame: &mut Frame, app: &App, area: Rect) {
    let layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(38), Constraint::Min(60)])
        .split(area);
    render_service_list(frame, app, layout[0]);

    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(13), Constraint::Min(10)])
        .split(layout[1]);
    render_service_summary(frame, app.selected_service(), app, right[0]);
    render_workspace_config(frame, app, right[1]);
}

fn render_output_page(frame: &mut Frame, app: &App, area: Rect) {
    let budget = area.height.saturating_sub(2) as usize;
    let lines = app.history_lines(budget.max(1));
    let text = if lines.is_empty() {
        Text::from(vec![Line::from(Span::styled(
            app.text("当前还没有输出。", "No output captured yet."),
            Style::default().fg(MUTED),
        ))])
    } else {
        Text::from(lines.into_iter().map(Line::from).collect::<Vec<_>>())
    };
    let paragraph = Paragraph::new(text)
        .block(panel(app.text("输出日志", "Output")))
        .style(Style::default().fg(TEXT))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

fn render_instance_list(frame: &mut Frame, app: &App, area: Rect) {
    if app.instances.is_empty() {
        let empty = Paragraph::new(Text::from(vec![
            Line::from(Span::styled(
                app.text("当前还没有已安装实例。", "No installed instances yet."),
                Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                app.text(
                    "按 `i` 执行首次安装，或按 `n` 直接新增实例。",
                    "Press `i` to initialize, or `n` to add one instance.",
                ),
                Style::default().fg(MUTED),
            )),
        ]))
        .block(panel(app.text("实例列表", "Instances")))
        .wrap(Wrap { trim: true });
        frame.render_widget(empty, area);
        return;
    }

    let items = app
        .instances
        .iter()
        .map(|instance| {
            let state = if instance.running {
                app.text("运行中", "RUNNING")
            } else {
                app.text("已停止", "STOPPED")
            };
            let reg = if instance.registered {
                app.text("已注册", "REGISTERED")
            } else {
                app.text("未注册", "UNREGISTERED")
            };
            ListItem::new(vec![
                Line::from(vec![
                    Span::styled(
                        format!("{} / {}", instance.service.label(), instance.instance_name),
                        Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
                    ),
                    Span::raw("  "),
                    Span::styled(state, instance_status_style(instance.running)),
                ]),
                Line::from(vec![
                    Span::styled(
                        format!("{}:{}   {}", instance.host, instance.port, reg),
                        Style::default().fg(MUTED),
                    ),
                    Span::raw("  "),
                    Span::styled(&instance.service_name, Style::default().fg(ACCENT)),
                ]),
            ])
        })
        .collect::<Vec<_>>();

    let list = List::new(items)
        .block(panel(app.text("实例列表", "Instances")))
        .highlight_style(
            Style::default()
                .bg(PANEL)
                .fg(TEXT)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(">> ");
    let mut state = ListState::default();
    state.select(Some(app.selected_instance_index));
    frame.render_stateful_widget(list, area, &mut state);
}

fn render_manager_summary(frame: &mut Frame, app: &App, area: Rect) {
    let update_lines = match &app.last_update_check {
        Some(check) => vec![
            kv(
                app.text("最新 release", "Latest Release"),
                check
                    .latest_release_tag
                    .clone()
                    .unwrap_or_else(|| app.text("未知", "unknown").to_string()),
            ),
            kv(
                app.text("二进制更新", "Binary Update"),
                if check.binary_update_available {
                    app.text("可更新", "available").to_string()
                } else {
                    app.text("已最新", "up to date").to_string()
                },
            ),
            kv(
                app.text("管理器更新", "Manager Update"),
                if check.manager_update_available {
                    app.text("可更新", "available").to_string()
                } else {
                    app.text("已最新", "up to date").to_string()
                },
            ),
        ],
        None => vec![kv(
            app.text("更新状态", "Update Status"),
            app.text("按 `c` 检查更新", "Press `c` to check updates")
                .to_string(),
        )],
    };

    let mut lines = vec![
        kv(
            app.text("初始化", "Initialized"),
            yes_no(app, app.manager.state.initialized).to_string(),
        ),
        kv(
            app.text("语言", "Language"),
            app.manager.state.language.clone(),
        ),
        kv(
            app.text("管理器", "Manager"),
            app.manager.state.manager_version.clone(),
        ),
        kv(
            app.text("安装目录", "Install Dir"),
            sanitize_display_text(&app.manager.paths.install_dir.display().to_string()),
        ),
        kv(
            app.text("LanceDB 根", "LanceDB Root"),
            sanitize_display_text(&app.manager.state.lancedb_root.display().to_string()),
        ),
        kv(
            app.text("DuckDB 根", "DuckDB Root"),
            sanitize_display_text(&app.manager.state.duckdb_root.display().to_string()),
        ),
    ];
    lines.extend(update_lines);

    let paragraph = Paragraph::new(Text::from(lines))
        .block(panel(app.text("管理状态", "Manager State")))
        .style(Style::default().fg(TEXT))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

fn render_selected_instance(frame: &mut Frame, app: &App, area: Rect) {
    let text = if let Some(instance) = app.selected_instance() {
        let mut lines = vec![
            kv(
                app.text("服务", "Service"),
                instance.service.label().to_string(),
            ),
            kv(
                app.text("实例名", "Instance"),
                instance.instance_name.clone(),
            ),
            kv(
                app.text("监听地址", "Listen"),
                format!("{}:{}", instance.host, instance.port),
            ),
            kv(
                app.text("数据路径", "Data Path"),
                sanitize_display_text(&instance.db_path.display().to_string()),
            ),
            kv(
                app.text("服务名", "Service Name"),
                instance.service_name.clone(),
            ),
            kv(
                app.text("注册状态", "Registered"),
                yes_no(app, instance.registered).to_string(),
            ),
            kv(
                app.text("运行状态", "Running"),
                yes_no(app, instance.running).to_string(),
            ),
            kv(
                app.text("配置文件", "Config"),
                sanitize_display_text(&instance.config_path.display().to_string()),
            ),
        ];
        if let Some(error) = app.last_manager_error.as_ref() {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{}: ", app.text("注意", "Warning")),
                    Style::default().fg(DANGER).add_modifier(Modifier::BOLD),
                ),
                Span::styled(error, Style::default().fg(TEXT)),
            ]));
        }
        Text::from(lines)
    } else {
        Text::from(vec![Line::from(Span::styled(
            app.text("未选中实例。", "No instance selected."),
            Style::default().fg(MUTED),
        ))])
    };
    let paragraph = Paragraph::new(text)
        .block(panel(app.text("选中实例", "Selected Instance")))
        .style(Style::default().fg(TEXT))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

fn render_instance_config(frame: &mut Frame, app: &App, area: Rect) {
    let budget = area.height.saturating_sub(2) as usize;
    let lines = app.instance_config_preview_lines(budget.max(1));
    let paragraph = Paragraph::new(Text::from(
        lines.into_iter().map(Line::from).collect::<Vec<_>>(),
    ))
    .block(panel(app.text("实例配置预览", "Instance Config")))
    .style(Style::default().fg(TEXT))
    .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

fn render_service_list(frame: &mut Frame, app: &App, area: Rect) {
    let items: Vec<ListItem> = app
        .services
        .iter()
        .map(|service| {
            let status = service.status();
            let port = service
                .config
                .as_ref()
                .map(|config| config.port.to_string())
                .unwrap_or_else(|| service.spec.default_port.to_string());
            ListItem::new(vec![
                Line::from(vec![
                    Span::styled(
                        service.spec.folder_name,
                        Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
                    ),
                    Span::raw("  "),
                    Span::styled(
                        workspace_status_label(app, status),
                        workspace_status_style(status),
                    ),
                ]),
                Line::from(vec![
                    Span::styled(
                        service_title(app, service.spec.id),
                        Style::default().fg(MUTED),
                    ),
                    Span::raw("  "),
                    Span::styled(format!(":{port}"), Style::default().fg(ACCENT)),
                ]),
            ])
        })
        .collect();
    let list = List::new(items)
        .block(panel(app.text("工作区服务", "Workspace Services")))
        .highlight_style(
            Style::default()
                .bg(PANEL)
                .fg(TEXT)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(">> ");
    let mut state = ListState::default();
    state.select(Some(app.selected_workspace_index));
    frame.render_stateful_widget(list, area, &mut state);
}

fn render_service_summary(frame: &mut Frame, service: &ServiceState, app: &App, area: Rect) {
    let status = service.status();
    let config = service.config.as_ref();
    let health_label = if service.last_probe_ok {
        app.text("可达", "reachable")
    } else {
        app.text("离线", "offline")
    };
    let build_line = match &service.last_build {
        Some(record) => format!(
            "{} {} {} {} (exit {})",
            record.profile,
            app.text("构建", "build"),
            if record.success {
                app.text("成功", "ok")
            } else {
                app.text("失败", "failed")
            },
            record.finished_at,
            format_exit_code(record.exit_code)
        ),
        None if service.build_running => {
            format!(
                "{} {} {}",
                app.build_profile,
                app.text("构建中", "build in progress"),
                app.spinner_frame()
            )
        }
        None => app
            .text("本次会话还没有执行 build", "No build in this session")
            .to_string(),
    };
    let process_line = match &service.managed_process {
        Some(process) => format!(
            "PID {}   {} {}   {} {}   {} {}",
            process.pid,
            app.text("运行时长", "Uptime"),
            service.uptime_label().unwrap_or_else(|| "0s".to_string()),
            app.text("启动于", "Started"),
            process.started_label,
            app.text("配置", "Profile"),
            process.profile
        ),
        None => app
            .text(
                "当前会话没有接管该进程",
                "Not managed by the current session",
            )
            .to_string(),
    };
    let last_exit = service
        .last_exit
        .as_ref()
        .map(|record| {
            format!(
                "{} {} {} {}",
                app.text("上次退出码", "Last exit"),
                format_exit_code(record.exit_code),
                app.text("时间", "at"),
                record.finished_at
            )
        })
        .unwrap_or_else(|| app.text("上次退出: 无", "Last exit: n/a").to_string());

    let mut lines = vec![
        Line::from(vec![
            Span::styled(
                service.spec.folder_name,
                Style::default()
                    .fg(TEXT)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            ),
            Span::raw("  "),
            Span::styled(
                workspace_status_label(app, status),
                workspace_status_style(status),
            ),
        ]),
        Line::from(Span::styled(
            service_description(app, service.spec.id),
            Style::default().fg(MUTED),
        )),
        Line::from(""),
        kv(
            app.text("Manifest", "Manifest"),
            sanitize_display_text(
                &service
                    .spec
                    .manifest_path(&app.workspace.root)
                    .display()
                    .to_string(),
            ),
        ),
        kv(
            app.text("Binary", "Binary"),
            sanitize_display_text(
                &service
                    .spec
                    .binary_path(&app.workspace.root, app.build_profile)
                    .display()
                    .to_string(),
            ),
        ),
        kv(
            app.text("Config", "Config"),
            config
                .map(|value| {
                    format!(
                        "{} ({})",
                        sanitize_display_text(&value.source_path.display().to_string()),
                        app.localized_source_label(value.source_label)
                    )
                })
                .unwrap_or_else(|| app.text("无配置", "No config available").to_string()),
        ),
        kv(
            app.text("Data", "Data"),
            config
                .map(|value| value.db_path.clone())
                .unwrap_or_else(|| service.spec.default_db_path.to_string()),
        ),
        kv(
            app.text("Listen", "Listen"),
            config
                .map(|value| format!("{}:{} ({health_label})", value.host, value.port))
                .unwrap_or_else(|| {
                    format!(
                        "{}:{}",
                        service.spec.default_host, service.spec.default_port
                    )
                }),
        ),
        kv(
            app.text("Log", "Log"),
            config
                .map(|value| {
                    format!(
                        "{} [dir {} | file {}]",
                        sanitize_display_text(&value.log_path.display().to_string()),
                        sanitize_display_text(&value.log_dir.display().to_string()),
                        value.log_file_name
                    )
                })
                .unwrap_or_else(|| app.text("未知", "unknown").to_string()),
        ),
        kv(app.text("Runtime", "Runtime"), process_line),
        kv(app.text("Build", "Build"), build_line),
        kv(app.text("Lifecycle", "Lifecycle"), last_exit),
    ];
    if let Some(error) = service.last_error.as_ref() {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled(
                format!("{}: ", app.text("注意", "Attention")),
                Style::default().fg(DANGER).add_modifier(Modifier::BOLD),
            ),
            Span::styled(error.clone(), Style::default().fg(TEXT)),
        ]));
    }
    let paragraph = Paragraph::new(Text::from(lines))
        .block(panel(app.text("工作区详情", "Workspace Detail")))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

fn render_workspace_config(frame: &mut Frame, app: &App, area: Rect) {
    let budget = area.height.saturating_sub(2) as usize;
    let lines = app.workspace_config_preview_lines(budget.max(1));
    let paragraph = Paragraph::new(Text::from(
        lines.into_iter().map(Line::from).collect::<Vec<_>>(),
    ))
    .block(panel(app.text("工作区配置预览", "Workspace Config")))
    .style(Style::default().fg(TEXT))
    .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

fn render_footer(frame: &mut Frame, app: &App, area: Rect) {
    let lines = match app.active_page {
        ActivePage::Installed => vec![
            Line::from(app.text(
                "导航: Tab/左右切页  上下选择实例",
                "Navigation: Tab/Left/Right switch page  Up/Down select instance",
            )),
            Line::from(app.text(
                "实例: `i` 首装  `n` 新增  `e` 修改  `d` 卸载",
                "Instances: `i` init  `n` install  `e` edit  `d` uninstall",
            )),
            Line::from(app.text(
                "运行: `s` 启动  `x` 停止  `a` 全部启动  `z` 全部停止",
                "Runtime: `s` start  `x` stop  `a` start all  `z` stop all",
            )),
            Line::from(app.text(
                "维护: `c` 检查更新  `u` 更新二进制  `t` 切换语言  `m` 刷新入口",
                "Maintenance: `c` check updates  `u` update binaries  `t` switch lang  `m` refresh launcher",
            )),
            Line::from(app.text(
                "清理: `r` 移除入口  `w` 卸载全部  `q` 退出",
                "Cleanup: `r` remove launcher  `w` uninstall all  `q` quit",
            )),
        ],
        ActivePage::Workspace => vec![
            Line::from(app.text(
                "导航: Tab/左右切页  上下选择服务",
                "Navigation: Tab/Left/Right switch page  Up/Down select service",
            )),
            Line::from(app.text(
                "工作区: `g` 配置  `b` 构建  `s` 启动  `x` 停止  `r` 重启",
                "Workspace: `g` config  `b` build  `s` start  `x` stop  `r` restart",
            )),
            Line::from(app.text(
                "批量: `p` debug/release  `a` 全部启动  `z` 全部停止  `q` 退出",
                "Batch: `p` debug/release  `a` start all  `z` stop all  `q` quit",
            )),
            Line::from(app.text(
                "说明: 这里只控制仓库内工作区进程，不影响已安装系统实例。",
                "Note: This page only manages workspace-local processes, not installed system instances.",
            )),
        ],
        ActivePage::Output => vec![
            Line::from(app.text(
                "导航: Tab/左右切页",
                "Navigation: Tab/Left/Right switch page",
            )),
            Line::from(app.text("操作: `q` 退出", "Action: `q` quit")),
        ],
    };
    let footer = Paragraph::new(Text::from(lines))
        .block(panel(app.text("快捷键", "Shortcuts")))
        .style(Style::default().fg(TEXT))
        .wrap(Wrap { trim: true });
    frame.render_widget(footer, area);
}

fn render_modal(frame: &mut Frame, app: &App, modal: &Modal) {
    match modal {
        Modal::Form(form) => render_form_modal(frame, app, form),
        Modal::Confirm(confirm) => render_confirm_modal(frame, app, confirm),
    }
}

fn render_form_modal(frame: &mut Frame, app: &App, form: &FormState) {
    let extra = if matches!(form.mode, FormMode::Configure(_)) {
        2u16
    } else {
        0u16
    };
    let height = (form.fields.len() as u16)
        .saturating_add(6 + extra)
        .min(frame.area().height);
    let width = 84.min(frame.area().width.saturating_sub(4));
    let popup = centered_rect(width, height, frame.area());
    frame.render_widget(Clear, popup);

    let mut lines = Vec::new();
    if let FormMode::Configure(instance) = &form.mode {
        lines.push(Line::from(Span::styled(
            format!(
                "target: {} / {}",
                instance.service.label(),
                instance.instance_name
            ),
            Style::default().fg(INFO).add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(""));
    }

    for (index, field) in form.fields.iter().enumerate() {
        let mut label = format!("{:>14}", app.field_label(field.key));
        if index == form.selected_index {
            label = format!("> {label}");
        } else {
            label = format!("  {label}");
        }

        let value = match &field.kind {
            FieldKind::Choice(_) => format!("{}  ◀ ▶", display_field_value(app, field)),
            _ => display_field_value(app, field),
        };
        let style = if index == form.selected_index {
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(TEXT)
        };
        lines.push(Line::from(vec![
            Span::styled(format!("{label}: "), Style::default().fg(INFO)),
            Span::styled(value, style),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        app.form_hint(form),
        Style::default().fg(MUTED),
    )));

    let title = app.form_title(form);
    let paragraph = Paragraph::new(Text::from(lines))
        .block(panel(&title))
        .style(Style::default().fg(TEXT))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, popup);
}

fn render_confirm_modal(frame: &mut Frame, app: &App, confirm: &ConfirmState) {
    let popup = centered_rect(
        72.min(frame.area().width.saturating_sub(4)),
        8,
        frame.area(),
    );
    frame.render_widget(Clear, popup);
    let paragraph = Paragraph::new(Text::from(vec![
        Line::from(Span::styled(
            &confirm.message,
            Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            app.text(
                "按 Enter / y 确认，Esc / n 取消。",
                "Press Enter / y to confirm, Esc / n to cancel.",
            ),
            Style::default().fg(MUTED),
        )),
    ]))
    .block(panel(&confirm.title))
    .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, popup);
}

fn display_field_value(app: &App, field: &crate::app::FormField) -> String {
    match field.value.as_str() {
        "vldb-lancedb" => app
            .text("LanceDB (vldb-lancedb)", "LanceDB (vldb-lancedb)")
            .to_string(),
        "vldb-duckdb" => app
            .text("DuckDB (vldb-duckdb)", "DuckDB (vldb-duckdb)")
            .to_string(),
        _ => field.value.clone(),
    }
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(area.height.saturating_sub(height) / 2),
            Constraint::Length(height),
            Constraint::Min(0),
        ])
        .split(area);
    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(area.width.saturating_sub(width) / 2),
            Constraint::Length(width),
            Constraint::Min(0),
        ])
        .split(vertical[1]);
    horizontal[1]
}

fn sanitize_display_text(value: &str) -> String {
    if let Some(rest) = value.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{rest}");
    }
    value.strip_prefix(r"\\?\").unwrap_or(value).to_string()
}

fn service_title(app: &App, service: crate::service::ServiceId) -> &'static str {
    match service {
        crate::service::ServiceId::LanceDb => app.text("向量网关", "Vector Gateway"),
        crate::service::ServiceId::DuckDb => app.text("SQL 网关", "SQL Gateway"),
    }
}

fn service_description(app: &App, service: crate::service::ServiceId) -> &'static str {
    match service {
        crate::service::ServiceId::LanceDb => app.text(
            "管理本地 LanceDB 向量表与语义检索。",
            "Manage local LanceDB vector tables and semantic search.",
        ),
        crate::service::ServiceId::DuckDb => app.text(
            "管理本地 DuckDB 分析查询与参数化 SQL 访问。",
            "Manage local DuckDB analytics and parameterized SQL access.",
        ),
    }
}

fn workspace_status_label(app: &App, status: ServiceStatus) -> &'static str {
    match status {
        ServiceStatus::Building => app.text("构建中", "BUILDING"),
        ServiceStatus::Starting => app.text("启动中", "STARTING"),
        ServiceStatus::Running => app.text("运行中", "RUNNING"),
        ServiceStatus::External => app.text("外部进程", "EXTERNAL"),
        ServiceStatus::Failed => app.text("失败", "FAILED"),
        ServiceStatus::Stopped => app.text("已停止", "STOPPED"),
    }
}

fn yes_no<'a>(app: &'a App, value: bool) -> &'a str {
    if value {
        app.text("是", "yes")
    } else {
        app.text("否", "no")
    }
}

fn instance_status_style(running: bool) -> Style {
    let color = if running { SUCCESS } else { MUTED };
    Style::default().fg(color).add_modifier(Modifier::BOLD)
}

fn workspace_status_style(status: ServiceStatus) -> Style {
    let color = match status {
        ServiceStatus::Building => WARN,
        ServiceStatus::Starting => ACCENT,
        ServiceStatus::Running => SUCCESS,
        ServiceStatus::External => EXTERNAL,
        ServiceStatus::Failed => DANGER,
        ServiceStatus::Stopped => MUTED,
    };
    Style::default().fg(color).add_modifier(Modifier::BOLD)
}

fn kv(label: &str, value: String) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label:>12}: "), Style::default().fg(INFO)),
        Span::styled(value, Style::default().fg(TEXT)),
    ])
}

fn panel(title: &str) -> Block<'_> {
    Block::default()
        .title(Span::styled(
            format!(" {title} "),
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(BORDER))
        .style(Style::default().bg(PANEL))
}
