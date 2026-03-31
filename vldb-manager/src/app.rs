use std::collections::VecDeque;
use std::fs;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::installed::{
    InitRequest, InstallManager, InstalledInstance, InstanceRequest, UpdateCheck,
};
use crate::service::{
    BackgroundEvent, BuildProfile, BuildRecord, ExitRecord, ServiceId, ServiceState, Workspace,
    ensure_workspace_config, format_exit_code, load_service_config, probe_service, spawn_build,
    start_service, stop_process, timestamp_label,
};

const MAX_HISTORY: usize = 600;
const SPINNER_FRAMES: [&str; 8] = ["⠁", "⠂", "⠄", "⠂", "⠁", "⠈", "⠐", "⠠"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivePage {
    Installed,
    Workspace,
    Output,
}

impl ActivePage {
    pub fn all() -> [Self; 3] {
        [Self::Installed, Self::Workspace, Self::Output]
    }
}

#[derive(Debug, Clone)]
pub enum Modal {
    Form(FormState),
    Confirm(ConfirmState),
}

#[derive(Debug, Clone)]
pub struct FormState {
    pub mode: FormMode,
    pub fields: Vec<FormField>,
    pub selected_index: usize,
}

impl FormState {
    pub fn previous_field(&mut self) {
        if self.selected_index == 0 {
            self.selected_index = self.fields.len().saturating_sub(1);
        } else {
            self.selected_index -= 1;
        }
    }

    pub fn next_field(&mut self) {
        self.selected_index = (self.selected_index + 1) % self.fields.len().max(1);
    }

    pub fn field_value(&self, key: FormFieldKey) -> Option<&str> {
        self.fields
            .iter()
            .find(|field| field.key == key)
            .map(|field| field.value.trim())
    }

    fn selected_field_mut(&mut self) -> Option<&mut FormField> {
        self.fields.get_mut(self.selected_index)
    }
}

#[derive(Debug, Clone)]
pub enum FormMode {
    Initialize,
    Install,
    Configure(InstalledInstance),
}

#[derive(Debug, Clone)]
pub struct FormField {
    pub key: FormFieldKey,
    pub kind: FieldKind,
    pub value: String,
}

#[derive(Debug, Clone)]
pub enum FieldKind {
    Text,
    Number,
    Choice(Vec<String>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormFieldKey {
    Service,
    InstanceName,
    BindHost,
    Port,
    DataPath,
    ServiceName,
    LanceDbRoot,
    DuckDbRoot,
    LanceDbPort,
    DuckDbPort,
    LanceDbServiceName,
    DuckDbServiceName,
}

#[derive(Debug, Clone)]
pub struct ConfirmState {
    pub title: String,
    pub message: String,
    pub action: ConfirmAction,
}

#[derive(Debug, Clone)]
pub enum ConfirmAction {
    UpdateBinaries,
    RemoveLauncher,
    UninstallInstance(InstalledInstance),
    UninstallAll,
}

pub struct App {
    pub workspace: Workspace,
    pub services: Vec<ServiceState>,
    pub selected_workspace_index: usize,
    pub manager: InstallManager,
    pub instances: Vec<InstalledInstance>,
    pub selected_instance_index: usize,
    pub active_page: ActivePage,
    pub build_profile: BuildProfile,
    pub history: VecDeque<String>,
    pub spinner_index: usize,
    pub launched_at: String,
    pub last_update_check: Option<UpdateCheck>,
    pub modal: Option<Modal>,
    pub last_manager_error: Option<String>,
    manager_exe: PathBuf,
    tx: Sender<BackgroundEvent>,
    rx: Receiver<BackgroundEvent>,
    should_quit: bool,
    last_refresh: Instant,
}

impl App {
    pub fn new() -> Result<Self> {
        let workspace = Workspace::discover()?;
        let services = workspace
            .services
            .iter()
            .copied()
            .map(ServiceState::new)
            .collect();
        let manager_exe = std::env::current_exe()?;
        let manager = InstallManager::load(&workspace.root)?;
        let (tx, rx) = mpsc::channel();

        let mut app = Self {
            workspace,
            services,
            selected_workspace_index: 0,
            manager,
            instances: Vec::new(),
            selected_instance_index: 0,
            active_page: ActivePage::Installed,
            build_profile: BuildProfile::Debug,
            history: VecDeque::new(),
            spinner_index: 0,
            launched_at: timestamp_label(chrono::Local::now()),
            last_update_check: None,
            modal: None,
            last_manager_error: None,
            manager_exe,
            tx,
            rx,
            should_quit: false,
            last_refresh: Instant::now() - Duration::from_secs(5),
        };

        app.push_history(
            "manager",
            app.text("VLDB 管理器会话已启动。", "VLDB manager session started."),
        );
        match app.manager.ensure_launcher(&app.manager_exe) {
            Ok(()) => app.push_history(
                "manager",
                app.text(
                    "已刷新 vldb 管理入口。",
                    "Refreshed the vldb manager launcher.",
                ),
            ),
            Err(error) => app.push_error(
                "manager",
                &format!(
                    "{}: {error}",
                    app.text("刷新启动入口失败", "failed to refresh launcher")
                ),
            ),
        }
        app.refresh_workspace_services();
        app.refresh_instances();
        Ok(app)
    }

    pub fn should_quit(&self) -> bool {
        self.should_quit
    }

    pub fn selected_service(&self) -> &ServiceState {
        &self.services[self.selected_workspace_index]
    }

    pub fn selected_instance(&self) -> Option<&InstalledInstance> {
        self.instances.get(self.selected_instance_index)
    }

    pub fn spinner_frame(&self) -> &'static str {
        SPINNER_FRAMES[self.spinner_index % SPINNER_FRAMES.len()]
    }

    pub fn uses_chinese(&self) -> bool {
        self.manager.uses_chinese()
    }

    pub fn text<'a>(&self, zh: &'a str, en: &'a str) -> &'a str {
        if self.uses_chinese() { zh } else { en }
    }

    pub fn page_label(&self, page: ActivePage) -> &'static str {
        match page {
            ActivePage::Installed => self.text("实例管理", "Instances"),
            ActivePage::Workspace => self.text("工作区开发", "Workspace"),
            ActivePage::Output => self.text("输出日志", "Output"),
        }
    }

    pub fn field_label(&self, key: FormFieldKey) -> &'static str {
        match key {
            FormFieldKey::Service => self.text("服务", "Service"),
            FormFieldKey::InstanceName => self.text("实例名", "Instance"),
            FormFieldKey::BindHost => self.text("绑定 IP", "Bind Host"),
            FormFieldKey::Port => self.text("端口", "Port"),
            FormFieldKey::DataPath => self.text("数据路径", "Data Path"),
            FormFieldKey::ServiceName => self.text("服务注册名", "Service Name"),
            FormFieldKey::LanceDbRoot => self.text("LanceDB 根目录", "LanceDB Root"),
            FormFieldKey::DuckDbRoot => self.text("DuckDB 根目录", "DuckDB Root"),
            FormFieldKey::LanceDbPort => self.text("LanceDB 端口", "LanceDB Port"),
            FormFieldKey::DuckDbPort => self.text("DuckDB 端口", "DuckDB Port"),
            FormFieldKey::LanceDbServiceName => self.text("LanceDB 服务名", "LanceDB Service Name"),
            FormFieldKey::DuckDbServiceName => self.text("DuckDB 服务名", "DuckDB Service Name"),
        }
    }

    pub fn form_title(&self, form: &FormState) -> String {
        match &form.mode {
            FormMode::Initialize => self.text("首次安装 VLDB", "Initialize VLDB").to_string(),
            FormMode::Install => self.text("安装单个实例", "Install Instance").to_string(),
            FormMode::Configure(instance) => format!(
                "{} {} / {}",
                self.text("修改实例配置", "Configure"),
                instance.service.label(),
                instance.instance_name
            ),
        }
    }

    pub fn form_hint(&self, form: &FormState) -> &'static str {
        match form.mode {
            FormMode::Initialize => self.text(
                "输入完成后按 Enter 执行首次安装，Esc 取消。",
                "Press Enter to initialize, Esc to cancel.",
            ),
            FormMode::Install => self.text(
                "上下选择字段，左右切换服务，Enter 安装实例。",
                "Use Up/Down to move, Left/Right to change service, Enter to install.",
            ),
            FormMode::Configure(_) => self.text(
                "修改 host/port/data path/service name，Enter 保存。",
                "Adjust host/port/data path/service name, then press Enter to save.",
            ),
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Result<()> {
        if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('c') {
            self.should_quit = true;
            return Ok(());
        }

        if self.modal.is_some() {
            return self.handle_modal_key(key);
        }

        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Left | KeyCode::BackTab => self.select_previous_page(),
            KeyCode::Right | KeyCode::Tab => self.select_next_page(),
            KeyCode::Char('1') => self.active_page = ActivePage::Installed,
            KeyCode::Char('2') => self.active_page = ActivePage::Workspace,
            KeyCode::Char('3') => self.active_page = ActivePage::Output,
            _ => match self.active_page {
                ActivePage::Installed => self.handle_installed_key(key),
                ActivePage::Workspace => self.handle_workspace_key(key),
                ActivePage::Output => {}
            },
        }

        Ok(())
    }

    pub fn tick(&mut self) {
        if self.last_refresh.elapsed() < Duration::from_millis(350) {
            return;
        }

        self.spinner_index = (self.spinner_index + 1) % SPINNER_FRAMES.len();
        self.refresh_workspace_services();
        self.refresh_instances();
        self.last_refresh = Instant::now();
    }

    pub fn drain_background_events(&mut self) {
        while let Ok(event) = self.rx.try_recv() {
            match event {
                BackgroundEvent::LogLine {
                    service,
                    scope,
                    line,
                } => {
                    let prefix = format!("{} {}", service.short_label(), scope.label());
                    self.push_history(&prefix, &line);
                }
                BackgroundEvent::BuildFinished {
                    service,
                    profile,
                    success,
                    exit_code,
                } => {
                    let build_failed_label =
                        self.text("构建失败，退出码", "Build failed with exit code");
                    if let Some(state) = self.service_state_mut(service) {
                        state.build_running = false;
                        state.build_started_at = None;
                        state.last_build = Some(BuildRecord {
                            profile,
                            finished_at: now_label(),
                            success,
                            exit_code,
                        });
                        if success {
                            state.last_error = None;
                        } else {
                            state.last_error = Some(format!(
                                "{} {}",
                                build_failed_label,
                                format_exit_code(exit_code)
                            ));
                        }
                    }

                    let status_label = if success {
                        self.text("成功", "succeeded")
                    } else {
                        self.text("失败", "failed")
                    };
                    self.push_history(
                        "build",
                        &format!(
                            "{} {profile} {} {status_label} (exit: {})",
                            service.label(),
                            self.text("构建", "build"),
                            format_exit_code(exit_code)
                        ),
                    );
                }
            }
        }
    }

    pub fn shutdown(&mut self) {
        let mut stopped = 0usize;
        for state in &mut self.services {
            if let Some(mut process) = state.managed_process.take()
                && stop_process(&mut process).is_ok()
            {
                stopped += 1;
            }
        }

        if stopped > 0 {
            self.push_history(
                "manager",
                &format!(
                    "{} {stopped} {}",
                    self.text("退出前已关闭", "Shut down"),
                    self.text(
                        "个由当前会话接管的工作区服务",
                        "managed workspace service(s) before exit"
                    )
                ),
            );
        }
    }

    pub fn workspace_config_preview_lines(&self, max_lines: usize) -> Vec<String> {
        let selected = self.selected_service();
        let Some(config) = selected.config.as_ref() else {
            return vec![
                self.text(
                    "当前工作区还没有配置文件。",
                    "No workspace config file found yet.",
                )
                .to_string(),
            ];
        };

        let mut lines = vec![
            format!(
                "{}: {} ({})",
                self.text("来源", "source"),
                sanitize_display_text(&config.source_path.display().to_string()),
                self.localized_source_label(config.source_label)
            ),
            String::new(),
        ];

        let raw_lines: Vec<String> = config.raw_text.lines().map(sanitize_display_text).collect();
        if raw_lines.is_empty() {
            lines.push(self.text("(空配置)", "(empty config)").to_string());
            return lines;
        }

        let content_budget = max_lines.saturating_sub(lines.len());
        if raw_lines.len() <= content_budget {
            lines.extend(raw_lines);
            return lines;
        }

        lines.extend(raw_lines.into_iter().take(content_budget.saturating_sub(1)));
        lines.push(
            self.text("... 预览已截断 ...", "... config preview truncated ...")
                .to_string(),
        );
        lines
    }

    pub fn instance_config_preview_lines(&self, max_lines: usize) -> Vec<String> {
        let Some(instance) = self.selected_instance() else {
            return vec![
                self.text(
                    "当前还没有已安装实例。",
                    "There are no installed instances yet.",
                )
                .to_string(),
            ];
        };

        let mut lines = vec![
            format!(
                "{}: {}",
                self.text("来源", "source"),
                sanitize_display_text(&instance.config_path.display().to_string())
            ),
            String::new(),
        ];
        match fs::read_to_string(&instance.config_path) {
            Ok(raw) => {
                let raw_lines: Vec<String> = raw.lines().map(sanitize_display_text).collect();
                if raw_lines.is_empty() {
                    lines.push(self.text("(空配置)", "(empty config)").to_string());
                    return lines;
                }
                let content_budget = max_lines.saturating_sub(lines.len());
                if raw_lines.len() <= content_budget {
                    lines.extend(raw_lines);
                    return lines;
                }
                lines.extend(raw_lines.into_iter().take(content_budget.saturating_sub(1)));
                lines.push(
                    self.text("... 预览已截断 ...", "... config preview truncated ...")
                        .to_string(),
                );
            }
            Err(error) => lines.push(format!(
                "{}: {error}",
                self.text("读取配置失败", "failed to read config")
            )),
        }
        lines
    }

    pub fn history_lines(&self, max_lines: usize) -> Vec<String> {
        let len = self.history.len();
        let start = len.saturating_sub(max_lines);
        self.history.iter().skip(start).cloned().collect()
    }

    pub fn localized_source_label<'a>(&self, label: &'a str) -> &'a str {
        match label {
            "workspace" => self.text("工作区", "workspace"),
            "example" => self.text("示例", "example"),
            _ => label,
        }
    }

    fn handle_installed_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.select_previous_instance(),
            KeyCode::Down | KeyCode::Char('j') => self.select_next_instance(),
            KeyCode::Char('t') => self.toggle_language_action(),
            KeyCode::Char('c') => self.check_updates_action(),
            KeyCode::Char('u') => self.open_confirm(
                self.text("更新应用二进制", "Update Service Binaries"),
                self.text(
                    "将按最新 release 更新已安装服务，并自动恢复之前正在运行的实例。",
                    "This updates installed service binaries to the latest release and restores running instances.",
                ),
                ConfirmAction::UpdateBinaries,
            ),
            KeyCode::Char('i') => self.open_initialize_form(),
            KeyCode::Char('n') => self.open_install_form(),
            KeyCode::Char('e') => self.open_configure_form(),
            KeyCode::Char('s') => self.start_selected_instance_action(),
            KeyCode::Char('x') => self.stop_selected_instance_action(),
            KeyCode::Char('a') => self.run_manager_op("manager", |manager| {
                manager.start_all_instances()
            }),
            KeyCode::Char('z') => self.run_manager_op("manager", |manager| {
                manager.stop_all_instances()
            }),
            KeyCode::Char('d') => self.confirm_uninstall_selected(),
            KeyCode::Char('m') => self.refresh_launcher_action(),
            KeyCode::Char('r') => self.open_confirm(
                self.text("移除管理入口", "Remove Manager Launcher"),
                self.text(
                    "这会移除 vldb 命令入口，但不会删除任何实例和数据。",
                    "This removes the vldb launcher only. Instances and data stay intact.",
                ),
                ConfirmAction::RemoveLauncher,
            ),
            KeyCode::Char('w') => self.open_confirm(
                self.text("卸载全部", "Uninstall All"),
                self.text(
                    "这会移除程序文件、服务注册和管理入口，但保留数据库目录。",
                    "This removes program files, service registration, and the launcher, while preserving database directories.",
                ),
                ConfirmAction::UninstallAll,
            ),
            _ => {}
        }
    }

    fn handle_workspace_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.select_previous_workspace_service(),
            KeyCode::Down | KeyCode::Char('j') => self.select_next_workspace_service(),
            KeyCode::Char('p') => self.toggle_profile(),
            KeyCode::Char('g') => {
                self.run_workspace_op("config", |app| app.generate_config_for_selected())
            }
            KeyCode::Char('b') => self.run_workspace_op("build", |app| app.build_selected()),
            KeyCode::Char('s') => self.run_workspace_op("runtime", |app| app.start_selected()),
            KeyCode::Char('x') => self.run_workspace_op("runtime", |app| app.stop_selected()),
            KeyCode::Char('r') => self.run_workspace_op("runtime", |app| app.restart_selected()),
            KeyCode::Char('a') => self.start_all_workspace(),
            KeyCode::Char('z') => self.stop_all_workspace(),
            _ => {}
        }
    }

    fn handle_modal_key(&mut self, key: KeyEvent) -> Result<()> {
        let Some(mut modal) = self.modal.take() else {
            return Ok(());
        };

        let keep_open = match &mut modal {
            Modal::Form(form) => self.handle_form_key(form, key)?,
            Modal::Confirm(confirm) => self.handle_confirm_key(confirm, key),
        };

        if keep_open {
            self.modal = Some(modal);
        }
        Ok(())
    }

    fn handle_form_key(&mut self, form: &mut FormState, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Esc => Ok(false),
            KeyCode::Up => {
                form.previous_field();
                Ok(true)
            }
            KeyCode::Down | KeyCode::Tab => {
                form.next_field();
                Ok(true)
            }
            KeyCode::BackTab => {
                form.previous_field();
                Ok(true)
            }
            KeyCode::Left => {
                self.adjust_choice_field(form, false);
                Ok(true)
            }
            KeyCode::Right => {
                self.adjust_choice_field(form, true);
                Ok(true)
            }
            KeyCode::Backspace => {
                if let Some(field) = form.selected_field_mut()
                    && !matches!(field.kind, FieldKind::Choice(_))
                {
                    field.value.pop();
                }
                Ok(true)
            }
            KeyCode::Enter => match self.submit_form(form) {
                Ok(keep_open) => Ok(keep_open),
                Err(error) => {
                    self.push_error("form", &error.to_string());
                    Ok(true)
                }
            },
            KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some(field) = form.selected_field_mut() {
                    match &field.kind {
                        FieldKind::Choice(_) => {}
                        FieldKind::Number if ch.is_ascii_digit() => field.value.push(ch),
                        FieldKind::Text => field.value.push(ch),
                        FieldKind::Number => {}
                    }
                }
                Ok(true)
            }
            _ => Ok(true),
        }
    }

    fn handle_confirm_key(&mut self, confirm: &mut ConfirmState, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Esc | KeyCode::Char('n') => false,
            KeyCode::Enter | KeyCode::Char('y') => {
                self.execute_confirm(confirm.action.clone());
                false
            }
            _ => true,
        }
    }

    fn submit_form(&mut self, form: &FormState) -> Result<bool> {
        match &form.mode {
            FormMode::Initialize => {
                let request = InitRequest {
                    lancedb_root: PathBuf::from(
                        form.field_value(FormFieldKey::LanceDbRoot)
                            .unwrap_or_default(),
                    ),
                    duckdb_root: PathBuf::from(
                        form.field_value(FormFieldKey::DuckDbRoot)
                            .unwrap_or_default(),
                    ),
                    bind_host: form
                        .field_value(FormFieldKey::BindHost)
                        .unwrap_or_default()
                        .to_string(),
                    lancedb_port: parse_port(
                        form.field_value(FormFieldKey::LanceDbPort)
                            .unwrap_or_default(),
                    )?,
                    duckdb_port: parse_port(
                        form.field_value(FormFieldKey::DuckDbPort)
                            .unwrap_or_default(),
                    )?,
                    lancedb_service_name: form
                        .field_value(FormFieldKey::LanceDbServiceName)
                        .unwrap_or_default()
                        .to_string(),
                    duckdb_service_name: form
                        .field_value(FormFieldKey::DuckDbServiceName)
                        .unwrap_or_default()
                        .to_string(),
                };
                self.run_manager_op("install", |manager| {
                    manager.initialize_installation(request)
                });
                Ok(false)
            }
            FormMode::Install => {
                let service =
                    parse_service_id(form.field_value(FormFieldKey::Service).unwrap_or_default())?;
                let instance_name = form
                    .field_value(FormFieldKey::InstanceName)
                    .unwrap_or_default()
                    .to_string();
                let request = InstanceRequest {
                    service,
                    instance_name: instance_name.clone(),
                    bind_host: form
                        .field_value(FormFieldKey::BindHost)
                        .unwrap_or_default()
                        .to_string(),
                    port: parse_port(form.field_value(FormFieldKey::Port).unwrap_or_default())?,
                    data_path: PathBuf::from(
                        form.field_value(FormFieldKey::DataPath).unwrap_or_default(),
                    ),
                    service_name: form
                        .field_value(FormFieldKey::ServiceName)
                        .unwrap_or_default()
                        .to_string(),
                };
                self.run_manager_op("install", |manager| {
                    manager.install_single_instance(request)
                });
                self.select_instance_by(service, &instance_name);
                Ok(false)
            }
            FormMode::Configure(original) => {
                let request = InstanceRequest {
                    service: original.service,
                    instance_name: original.instance_name.clone(),
                    bind_host: form
                        .field_value(FormFieldKey::BindHost)
                        .unwrap_or_default()
                        .to_string(),
                    port: parse_port(form.field_value(FormFieldKey::Port).unwrap_or_default())?,
                    data_path: PathBuf::from(
                        form.field_value(FormFieldKey::DataPath).unwrap_or_default(),
                    ),
                    service_name: form
                        .field_value(FormFieldKey::ServiceName)
                        .unwrap_or_default()
                        .to_string(),
                };
                let original = original.clone();
                self.run_manager_op("config", |manager| {
                    manager.configure_instance(&original, request)
                });
                self.select_instance_by(original.service, &original.instance_name);
                Ok(false)
            }
        }
    }

    fn adjust_choice_field(&mut self, form: &mut FormState, forward: bool) {
        let Some(field) = form.selected_field_mut() else {
            return;
        };
        let FieldKind::Choice(options) = &field.kind else {
            return;
        };
        if options.is_empty() {
            return;
        }

        let current = options
            .iter()
            .position(|value| value == &field.value)
            .unwrap_or_default();
        let next = if forward {
            (current + 1) % options.len()
        } else if current == 0 {
            options.len() - 1
        } else {
            current - 1
        };
        field.value = options[next].clone();

        if matches!(form.mode, FormMode::Install) {
            self.refresh_install_defaults(form);
        }
    }

    fn refresh_install_defaults(&self, form: &mut FormState) {
        let service = parse_service_id(form.field_value(FormFieldKey::Service).unwrap_or_default())
            .unwrap_or(ServiceId::LanceDb);
        let instance_name = form
            .field_value(FormFieldKey::InstanceName)
            .unwrap_or_default()
            .trim()
            .to_string();
        let instance_name = if instance_name.is_empty() {
            self.suggested_instance_name(service)
        } else {
            instance_name
        };
        let default_path = self
            .manager
            .default_instance_data_path(service, &instance_name)
            .display()
            .to_string();
        let default_service_name = self
            .manager
            .new_unique_service_name(service, &instance_name, None, None)
            .unwrap_or_else(|_| format!("{}-{instance_name}", service.label()));

        set_form_value(form, FormFieldKey::DataPath, default_path);
        set_form_value(form, FormFieldKey::ServiceName, default_service_name);
    }

    fn execute_confirm(&mut self, action: ConfirmAction) {
        match action {
            ConfirmAction::UpdateBinaries => {
                self.run_manager_op("update", |manager| manager.update_binaries_to_latest())
            }
            ConfirmAction::RemoveLauncher => {
                let result = self.manager.remove_launcher_only();
                match result {
                    Ok(()) => self.push_history(
                        "manager",
                        self.text(
                            "已移除 vldb 管理入口。",
                            "Removed the vldb manager launcher.",
                        ),
                    ),
                    Err(error) => self.push_error(
                        "manager",
                        &format!(
                            "{}: {error}",
                            self.text("移除管理入口失败", "failed to remove launcher")
                        ),
                    ),
                }
                self.refresh_instances();
            }
            ConfirmAction::UninstallInstance(instance) => self
                .run_manager_op("uninstall", |manager| {
                    manager.uninstall_single_instance(&instance)
                }),
            ConfirmAction::UninstallAll => {
                self.run_manager_op("uninstall", |manager| manager.uninstall_all())
            }
        }
    }

    fn refresh_workspace_services(&mut self) {
        let mut pending_history = Vec::new();
        let use_zh = self.uses_chinese();
        let poll_error_label = self.text("轮询进程状态失败", "Failed to poll process");

        for state in &mut self.services {
            match load_service_config(state.spec, &self.workspace.root) {
                Ok(config) => {
                    state.last_probe_ok = probe_service(&config);
                    state.config = Some(config);
                }
                Err(error) => {
                    state.config = None;
                    state.last_probe_ok = false;
                    state.last_error = Some(error.to_string());
                }
            }

            if let Some(process) = state.managed_process.as_mut() {
                match process.child.try_wait() {
                    Ok(Some(status)) => {
                        let exit_code = status.code();
                        state.last_exit = Some(ExitRecord {
                            finished_at: now_label(),
                            exit_code,
                        });
                        state.last_error = if status.success() {
                            None
                        } else {
                            Some(if use_zh {
                                format!(
                                    "{} 意外退出，退出码 {}",
                                    state.spec.folder_name,
                                    format_exit_code(exit_code)
                                )
                            } else {
                                format!(
                                    "{} exited unexpectedly with {}",
                                    state.spec.folder_name,
                                    format_exit_code(exit_code)
                                )
                            })
                        };
                        pending_history.push(if use_zh {
                            format!(
                                "{} 进程已退出，退出码 {}",
                                state.spec.folder_name,
                                format_exit_code(exit_code)
                            )
                        } else {
                            format!(
                                "{} process exited with {}",
                                state.spec.folder_name,
                                format_exit_code(exit_code)
                            )
                        });
                        state.managed_process = None;
                    }
                    Ok(None) => {}
                    Err(error) => {
                        state.last_error = Some(format!("{}: {error}", poll_error_label));
                        state.managed_process = None;
                    }
                }
            }
        }

        for line in pending_history {
            self.push_history("runtime", &line);
        }
    }

    fn refresh_instances(&mut self) {
        match self.manager.list_instances() {
            Ok(instances) => {
                self.instances = instances;
                self.last_manager_error = None;
                if self.selected_instance_index >= self.instances.len() {
                    self.selected_instance_index = self.instances.len().saturating_sub(1);
                }
            }
            Err(error) => {
                self.last_manager_error = Some(error.to_string());
            }
        }

        if let Ok(initialized) = self.manager.is_initialized() {
            self.manager.state.initialized = initialized;
        }
    }

    fn select_previous_page(&mut self) {
        let pages = ActivePage::all();
        let index = pages
            .iter()
            .position(|page| *page == self.active_page)
            .unwrap_or_default();
        let previous = if index == 0 {
            pages.len() - 1
        } else {
            index - 1
        };
        self.active_page = pages[previous];
    }

    fn select_next_page(&mut self) {
        let pages = ActivePage::all();
        let index = pages
            .iter()
            .position(|page| *page == self.active_page)
            .unwrap_or_default();
        self.active_page = pages[(index + 1) % pages.len()];
    }

    fn select_previous_instance(&mut self) {
        if self.instances.is_empty() {
            self.selected_instance_index = 0;
            return;
        }
        if self.selected_instance_index == 0 {
            self.selected_instance_index = self.instances.len() - 1;
        } else {
            self.selected_instance_index -= 1;
        }
    }

    fn select_next_instance(&mut self) {
        if self.instances.is_empty() {
            self.selected_instance_index = 0;
            return;
        }
        self.selected_instance_index = (self.selected_instance_index + 1) % self.instances.len();
    }

    fn select_instance_by(&mut self, service: ServiceId, instance_name: &str) {
        if let Some(index) = self
            .instances
            .iter()
            .position(|item| item.service == service && item.instance_name == instance_name)
        {
            self.selected_instance_index = index;
        }
    }

    fn select_previous_workspace_service(&mut self) {
        if self.selected_workspace_index == 0 {
            self.selected_workspace_index = self.services.len().saturating_sub(1);
        } else {
            self.selected_workspace_index -= 1;
        }
    }

    fn select_next_workspace_service(&mut self) {
        self.selected_workspace_index = (self.selected_workspace_index + 1) % self.services.len();
    }

    fn toggle_language_action(&mut self) {
        match self.manager.toggle_language() {
            Ok(message) => self.push_history("manager", &message),
            Err(error) => self.push_error(
                "manager",
                &format!(
                    "{}: {error}",
                    self.text("切换语言失败", "failed to switch language")
                ),
            ),
        }
    }

    fn check_updates_action(&mut self) {
        match self.manager.check_updates() {
            Ok(check) => {
                let latest = check
                    .latest_release_tag
                    .as_deref()
                    .unwrap_or(self.text("未知", "unknown"));
                let installed = check
                    .installed_release_tag
                    .as_deref()
                    .unwrap_or(self.text("未安装", "not installed"));
                self.push_history(
                    "update",
                    &format!(
                        "{} manager={} latest={} binaries={}",
                        self.text("更新检查完成", "Update check complete"),
                        check.current_manager_version,
                        latest,
                        installed
                    ),
                );
                self.last_update_check = Some(check);
            }
            Err(error) => self.push_error(
                "update",
                &format!(
                    "{}: {error}",
                    self.text("检查更新失败", "failed to check updates")
                ),
            ),
        }
    }

    fn refresh_launcher_action(&mut self) {
        match self.manager.ensure_launcher(&self.manager_exe) {
            Ok(()) => self.push_history(
                "manager",
                self.text(
                    "已刷新当前 vldb 管理命令入口。",
                    "Refreshed the current vldb launcher.",
                ),
            ),
            Err(error) => self.push_error(
                "manager",
                &format!(
                    "{}: {error}",
                    self.text("刷新管理入口失败", "failed to refresh launcher")
                ),
            ),
        }
    }

    fn open_initialize_form(&mut self) {
        if self.manager.state.initialized {
            self.push_history(
                "manager",
                self.text(
                    "已经存在实例配置，请直接新增实例或修改现有实例。",
                    "The installation is already initialized. Add or edit instances instead.",
                ),
            );
            return;
        }

        let lance_name = self
            .manager
            .new_unique_service_name(ServiceId::LanceDb, "default", None, None)
            .unwrap_or_else(|_| "vldb-lancedb-default".to_string());
        let duck_name = self
            .manager
            .new_unique_service_name(ServiceId::DuckDb, "default", None, None)
            .unwrap_or_else(|_| "vldb-duckdb-default".to_string());

        self.modal = Some(Modal::Form(FormState {
            mode: FormMode::Initialize,
            fields: vec![
                text_field(
                    FormFieldKey::LanceDbRoot,
                    self.manager.state.lancedb_root.display().to_string(),
                ),
                text_field(
                    FormFieldKey::DuckDbRoot,
                    self.manager.state.duckdb_root.display().to_string(),
                ),
                text_field(FormFieldKey::BindHost, "127.0.0.1".to_string()),
                number_field(FormFieldKey::LanceDbPort, "19301".to_string()),
                number_field(FormFieldKey::DuckDbPort, "19401".to_string()),
                text_field(FormFieldKey::LanceDbServiceName, lance_name),
                text_field(FormFieldKey::DuckDbServiceName, duck_name),
            ],
            selected_index: 0,
        }));
    }

    fn open_install_form(&mut self) {
        let service = self
            .selected_instance()
            .map(|instance| instance.service)
            .unwrap_or(ServiceId::LanceDb);
        let instance_name = self.suggested_instance_name(service);
        let port = self.suggested_port(service);
        let host = self
            .selected_instance()
            .filter(|instance| instance.service == service)
            .map(|instance| instance.host.clone())
            .unwrap_or_else(|| "127.0.0.1".to_string());
        let data_path = self
            .manager
            .default_instance_data_path(service, &instance_name)
            .display()
            .to_string();
        let service_name = self
            .manager
            .new_unique_service_name(service, &instance_name, None, None)
            .unwrap_or_else(|_| format!("{}-{instance_name}", service.label()));

        self.modal = Some(Modal::Form(FormState {
            mode: FormMode::Install,
            fields: vec![
                choice_field(
                    FormFieldKey::Service,
                    vec![
                        ServiceId::LanceDb.label().to_string(),
                        ServiceId::DuckDb.label().to_string(),
                    ],
                    service.label().to_string(),
                ),
                text_field(FormFieldKey::InstanceName, instance_name),
                text_field(FormFieldKey::BindHost, host),
                number_field(FormFieldKey::Port, port.to_string()),
                text_field(FormFieldKey::DataPath, data_path),
                text_field(FormFieldKey::ServiceName, service_name),
            ],
            selected_index: 0,
        }));
    }

    fn open_configure_form(&mut self) {
        let Some(instance) = self.selected_instance().cloned() else {
            self.push_history(
                "config",
                self.text(
                    "当前没有可修改的实例。",
                    "There is no installed instance to configure.",
                ),
            );
            return;
        };

        self.modal = Some(Modal::Form(FormState {
            mode: FormMode::Configure(instance.clone()),
            fields: vec![
                text_field(FormFieldKey::BindHost, instance.host),
                number_field(FormFieldKey::Port, instance.port.to_string()),
                text_field(
                    FormFieldKey::DataPath,
                    instance.db_path.display().to_string(),
                ),
                text_field(FormFieldKey::ServiceName, instance.service_name),
            ],
            selected_index: 0,
        }));
    }

    fn confirm_uninstall_selected(&mut self) {
        let Some(instance) = self.selected_instance().cloned() else {
            self.push_history(
                "uninstall",
                self.text(
                    "当前没有可卸载的实例。",
                    "There is no installed instance to remove.",
                ),
            );
            return;
        };

        self.open_confirm(
            self.text("卸载实例", "Uninstall Instance"),
            &format!(
                "{} {} / {}",
                self.text("确认卸载实例", "Uninstall"),
                instance.service.label(),
                instance.instance_name
            ),
            ConfirmAction::UninstallInstance(instance),
        );
    }

    fn open_confirm(&mut self, title: &str, message: &str, action: ConfirmAction) {
        self.modal = Some(Modal::Confirm(ConfirmState {
            title: title.to_string(),
            message: message.to_string(),
            action,
        }));
    }

    fn suggested_instance_name(&self, service: ServiceId) -> String {
        let mut index = 2usize;
        loop {
            let candidate = if self.instances.is_empty() && !self.manager.state.initialized {
                "default".to_string()
            } else {
                format!("instance-{index}")
            };
            let exists = self
                .instances
                .iter()
                .any(|instance| instance.service == service && instance.instance_name == candidate);
            if !exists {
                return candidate;
            }
            index += 1;
        }
    }

    fn suggested_port(&self, service: ServiceId) -> u16 {
        let mut candidate = match service {
            ServiceId::LanceDb => 19301u16,
            ServiceId::DuckDb => 19401u16,
        };

        loop {
            let in_use = self
                .instances
                .iter()
                .any(|instance| instance.port == candidate);
            if !in_use {
                return candidate;
            }
            candidate = candidate.saturating_add(1);
        }
    }

    fn start_selected_instance_action(&mut self) {
        let Some(instance) = self.selected_instance().cloned() else {
            self.push_history(
                "runtime",
                self.text(
                    "当前没有可启动的实例。",
                    "There is no installed instance to start.",
                ),
            );
            return;
        };
        self.run_manager_op("runtime", |manager| {
            manager.start_registered_instance(&instance)
        });
    }

    fn stop_selected_instance_action(&mut self) {
        let Some(instance) = self.selected_instance().cloned() else {
            self.push_history(
                "runtime",
                self.text(
                    "当前没有可停止的实例。",
                    "There is no installed instance to stop.",
                ),
            );
            return;
        };
        self.run_manager_op("runtime", |manager| {
            manager.stop_registered_instance(&instance)
        });
    }

    fn run_manager_op<F>(&mut self, source: &str, op: F)
    where
        F: FnOnce(&mut InstallManager) -> Result<String>,
    {
        match op(&mut self.manager) {
            Ok(message) => self.push_history(source, &message),
            Err(error) => self.push_error(source, &error.to_string()),
        }
        self.refresh_instances();
    }

    fn run_workspace_op<F>(&mut self, source: &str, op: F)
    where
        F: FnOnce(&mut Self) -> Result<()>,
    {
        if let Err(error) = op(self) {
            self.push_error(source, &error.to_string());
        }
    }

    fn start_all_workspace(&mut self) {
        for index in 0..self.services.len() {
            if let Err(error) = self.start_service_at(index) {
                self.push_error(
                    "runtime",
                    &format!(
                        "{}: {error}",
                        self.text("全部启动警告", "Start-all warning")
                    ),
                );
            }
        }
    }

    fn stop_all_workspace(&mut self) {
        for index in 0..self.services.len() {
            if let Err(error) = self.stop_service_at(index) {
                self.push_error(
                    "runtime",
                    &format!("{}: {error}", self.text("全部停止警告", "Stop-all warning")),
                );
            }
        }
    }

    fn toggle_profile(&mut self) {
        self.build_profile = self.build_profile.toggle();
        self.push_history(
            "manager",
            &format!(
                "{} {}",
                self.text("当前构建配置已切换到", "Active build profile switched to"),
                self.build_profile
            ),
        );
    }

    fn generate_config_for_selected(&mut self) -> Result<()> {
        let spec = self.selected_service().spec;
        let path = ensure_workspace_config(spec, &self.workspace.root)?;
        self.push_history(
            spec.folder_name,
            &format!(
                "{} {}",
                self.text("工作区配置已就绪:", "Workspace config ready at"),
                path.display()
            ),
        );
        self.refresh_workspace_services();
        Ok(())
    }

    fn build_selected(&mut self) -> Result<()> {
        let index = self.selected_workspace_index;
        let spec = self.services[index].spec;
        let state = &mut self.services[index];

        if state.build_running {
            self.push_history(
                spec.folder_name,
                self.text("构建已经在进行中。", "Build is already in progress."),
            );
            return Ok(());
        }

        if state.managed_process.is_some() {
            return Err(anyhow!(
                "{}",
                if self.uses_chinese() {
                    format!("请先停止 {}，再在当前平台上重新构建", spec.folder_name)
                } else {
                    format!(
                        "Stop {} before rebuilding on this platform",
                        spec.folder_name
                    )
                }
            ));
        }

        state.build_running = true;
        state.build_started_at = Some(Instant::now());
        state.last_error = None;

        spawn_build(
            spec,
            &self.workspace.root,
            self.build_profile,
            self.tx.clone(),
        );
        self.push_history(
            spec.folder_name,
            &format!(
                "{} {} {}",
                self.text("已开始", "Started"),
                self.build_profile,
                self.text("构建", "build")
            ),
        );
        Ok(())
    }

    fn start_selected(&mut self) -> Result<()> {
        self.start_service_at(self.selected_workspace_index)
    }

    fn stop_selected(&mut self) -> Result<()> {
        self.stop_service_at(self.selected_workspace_index)
    }

    fn restart_selected(&mut self) -> Result<()> {
        let index = self.selected_workspace_index;
        self.stop_service_at(index)?;
        self.start_service_at(index)
    }

    fn start_service_at(&mut self, index: usize) -> Result<()> {
        let spec = self.services[index].spec;
        let use_zh = self.uses_chinese();
        let state = &mut self.services[index];

        if state.build_running {
            return Err(anyhow!(
                "{}",
                if self.uses_chinese() {
                    format!("{} 仍在构建中", spec.folder_name)
                } else {
                    format!("{} is still building", spec.folder_name)
                }
            ));
        }
        if state.managed_process.is_some() {
            self.push_history(
                spec.folder_name,
                self.text(
                    "该服务已由当前会话接管。",
                    "This service is already managed by the current session.",
                ),
            );
            return Ok(());
        }
        if state.last_probe_ok {
            return Err(anyhow!(
                "{}",
                if use_zh {
                    format!(
                        "{} 已在端口 {} 上响应，可能由外部进程启动",
                        spec.folder_name,
                        state
                            .config
                            .as_ref()
                            .map(|value| value.port)
                            .unwrap_or(spec.default_port)
                    )
                } else {
                    format!(
                        "{} already responds on port {}, likely from an external process",
                        spec.folder_name,
                        state
                            .config
                            .as_ref()
                            .map(|value| value.port)
                            .unwrap_or(spec.default_port)
                    )
                }
            ));
        }

        let config_path = ensure_workspace_config(spec, &self.workspace.root)?;
        let process = start_service(
            spec,
            &self.workspace.root,
            self.build_profile,
            self.tx.clone(),
        )?;
        let pid = process.pid;

        state.managed_process = Some(process);
        state.last_error = None;

        self.push_history(
            spec.folder_name,
            &format!(
                "{} PID {pid} {} {} ({})",
                self.text("已启动，", "Started with"),
                self.text("使用配置", "using"),
                self.build_profile,
                config_path.display()
            ),
        );
        Ok(())
    }

    fn stop_service_at(&mut self, index: usize) -> Result<()> {
        let spec = self.services[index].spec;
        let state = &mut self.services[index];

        let Some(mut process) = state.managed_process.take() else {
            if state.last_probe_ok {
                return Err(anyhow!(
                    "{}",
                    if self.uses_chinese() {
                        format!(
                            "{} 由外部进程运行，管理器只能停止它自己启动的进程",
                            spec.folder_name
                        )
                    } else {
                        format!(
                            "{} is running externally; the manager can only stop processes it launched",
                            spec.folder_name
                        )
                    }
                ));
            }

            self.push_history(
                spec.folder_name,
                self.text("服务已经停止。", "Service is already stopped."),
            );
            return Ok(());
        };

        let exit_code = stop_process(&mut process)?;
        state.last_exit = Some(ExitRecord {
            finished_at: now_label(),
            exit_code,
        });
        state.last_error = None;
        self.push_history(
            spec.folder_name,
            &format!(
                "{} {} {}",
                self.text("已停止进程", "Stopped process"),
                process.pid,
                format_exit_code(exit_code)
            ),
        );
        Ok(())
    }

    fn service_state_mut(&mut self, service: ServiceId) -> Option<&mut ServiceState> {
        self.services
            .iter_mut()
            .find(|state| state.spec.id == service)
    }

    fn push_history(&mut self, source: &str, message: &str) {
        let line = format!("[{}] {:<12} {}", now_label(), source, message);
        self.history.push_back(line);
        while self.history.len() > MAX_HISTORY {
            self.history.pop_front();
        }
    }

    fn push_error(&mut self, source: &str, message: &str) {
        self.push_history(
            source,
            &format!("{}: {message}", self.text("错误", "ERROR")),
        );
    }
}

fn now_label() -> String {
    timestamp_label(chrono::Local::now())
}

fn text_field(key: FormFieldKey, value: String) -> FormField {
    FormField {
        key,
        kind: FieldKind::Text,
        value,
    }
}

fn number_field(key: FormFieldKey, value: String) -> FormField {
    FormField {
        key,
        kind: FieldKind::Number,
        value,
    }
}

fn choice_field(key: FormFieldKey, options: Vec<String>, value: String) -> FormField {
    FormField {
        key,
        kind: FieldKind::Choice(options),
        value,
    }
}

fn set_form_value(form: &mut FormState, key: FormFieldKey, value: String) {
    if let Some(field) = form.fields.iter_mut().find(|field| field.key == key) {
        field.value = value;
    }
}

fn sanitize_display_text(value: &str) -> String {
    if let Some(rest) = value.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{rest}");
    }
    value.strip_prefix(r"\\?\").unwrap_or(value).to_string()
}

fn parse_service_id(value: &str) -> Result<ServiceId> {
    match value.trim() {
        "vldb-lancedb" => Ok(ServiceId::LanceDb),
        "vldb-duckdb" => Ok(ServiceId::DuckDb),
        other => Err(anyhow!("未知服务 / Unknown service: {other}")),
    }
}

fn parse_port(value: &str) -> Result<u16> {
    value
        .trim()
        .parse::<u16>()
        .map_err(|error| anyhow!("端口无效 / Invalid port `{}`: {error}", value.trim()))
}
