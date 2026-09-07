//! Worktree choices for local, feature-enabled session commands.

use super::*;
use crate::app_event::ManagedWorktreeMode;
use crate::worktree_browser::Action;
use crate::worktree_browser::Entry;
use crate::worktree_browser::Request;

const BROWSER_VIEW_ID: &str = "managed-worktrees";

impl ChatWidget {
    pub(super) fn managed_worktree_available(&self) -> bool {
        self.config.features.enabled(Feature::Worktrees)
            && self.local_worktree_operations
            && get_git_repo_root(self.config.cwd.as_path()).is_some()
    }

    pub(super) fn show_session_checkout_picker(
        &mut self,
        mode: ManagedWorktreeMode,
        name: Option<String>,
    ) {
        if !self.managed_worktree_available() {
            match mode {
                ManagedWorktreeMode::New => {
                    self.app_event_tx.send(AppEvent::NewSession { name });
                }
                ManagedWorktreeMode::Fork => {
                    self.app_event_tx
                        .send(AppEvent::ForkCurrentSession { name });
                }
            }
            return;
        }

        let title = match mode {
            ManagedWorktreeMode::New => "Where should the new conversation run?",
            ManagedWorktreeMode::Fork => "Where should the forked conversation run?",
        };
        let current_name = name.clone();
        self.bottom_pane.show_selection_view(SelectionViewParams {
            title: Some(title.to_string()),
            footer_hint: Some(standard_popup_hint_line()),
            items: vec![
                SelectionItem {
                    name: "Current checkout".to_string(),
                    description: Some("Keep using the current working directory".to_string()),
                    actions: vec![Box::new(move |tx| match mode {
                        ManagedWorktreeMode::New => {
                            tx.send(AppEvent::NewSession {
                                name: current_name.clone(),
                            });
                        }
                        ManagedWorktreeMode::Fork => {
                            tx.send(AppEvent::ForkCurrentSession {
                                name: current_name.clone(),
                            });
                        }
                    })],
                    dismiss_on_select: true,
                    ..Default::default()
                },
                SelectionItem {
                    name: "New worktree".to_string(),
                    description: Some("Create an isolated managed checkout".to_string()),
                    actions: vec![Box::new(move |tx| {
                        tx.send(AppEvent::StartManagedWorktree {
                            mode,
                            name: name.clone(),
                        });
                    })],
                    dismiss_on_select: true,
                    ..Default::default()
                },
            ],
            ..Default::default()
        });
        self.request_redraw();
    }

    pub(super) fn show_managed_worktree_picker(&mut self) {
        if !self.config.features.enabled(Feature::Worktrees) {
            self.add_error_message(
                "Enable worktrees in /experimental to create a worktree.".to_string(),
            );
            return;
        }
        if !self.managed_worktree_available() {
            self.add_error_message("Managed worktrees require a local Git repository.".to_string());
            return;
        }

        self.bottom_pane.show_selection_view(SelectionViewParams {
            title: Some("Worktrees".to_string()),
            footer_hint: Some(standard_popup_hint_line()),
            items: vec![
                SelectionItem {
                    name: "Continue current conversation".to_string(),
                    description: Some("Preserve this conversation in the new checkout".to_string()),
                    actions: vec![Box::new(|tx| {
                        tx.send(AppEvent::StartManagedWorktree {
                            mode: ManagedWorktreeMode::Fork,
                            name: None,
                        });
                    })],
                    dismiss_on_select: true,
                    ..Default::default()
                },
                SelectionItem {
                    name: "Start new conversation".to_string(),
                    description: Some("Open a fresh conversation in the new checkout".to_string()),
                    actions: vec![Box::new(|tx| {
                        tx.send(AppEvent::StartManagedWorktree {
                            mode: ManagedWorktreeMode::New,
                            name: None,
                        });
                    })],
                    dismiss_on_select: true,
                    ..Default::default()
                },
                SelectionItem {
                    name: "Browse worktrees".to_string(),
                    description: Some(
                        "Resume an owner thread or copy a working directory".to_string(),
                    ),
                    actions: vec![Box::new(|tx| tx.send(AppEvent::BrowseManagedWorktrees))],
                    dismiss_on_select: true,
                    ..Default::default()
                },
            ],
            ..Default::default()
        });
        self.request_redraw();
    }

    pub(crate) fn request_managed_worktrees(&mut self) -> Option<Request> {
        if !self.managed_worktree_available() {
            return None;
        }
        let request = Request {
            id: uuid::Uuid::new_v4(),
            cwd: self.config.cwd.to_path_buf(),
            thread_id: self.thread_id,
        };
        self.bottom_pane.dismiss_view_by_id(BROWSER_VIEW_ID);
        self.worktree_popup_request_id = Some(request.id);
        self.bottom_pane.show_selection_view(SelectionViewParams {
            view_id: Some(BROWSER_VIEW_ID),
            title: Some("Managed worktrees".to_string()),
            items: vec![SelectionItem {
                name: "Loading worktrees…".to_string(),
                is_disabled: true,
                ..Default::default()
            }],
            footer_hint: Some(standard_popup_hint_line()),
            ..Default::default()
        });
        Some(request)
    }

    fn worktree_request_is_current(&self, request: &Request) -> bool {
        self.worktree_popup_request_id == Some(request.id)
            && request.cwd == self.config.cwd.as_path()
            && request.thread_id == self.thread_id
            && self.managed_worktree_available()
    }

    pub(crate) fn on_managed_worktrees_loaded(
        &mut self,
        request: Request,
        result: Result<Vec<Entry>, String>,
    ) {
        if self.worktree_popup_request_id != Some(request.id) {
            return;
        }
        if !self.worktree_request_is_current(&request)
            || !self.bottom_pane.dismiss_active_view_if_id(BROWSER_VIEW_ID)
        {
            self.worktree_popup_request_id = None;
            self.bottom_pane.dismiss_view_by_id(BROWSER_VIEW_ID);
            return;
        }
        let entries = match result {
            Ok(entries) => entries,
            Err(error) => {
                self.worktree_popup_request_id = None;
                self.add_error_message(format!("Cannot list managed worktrees: {error}"));
                return;
            }
        };
        self.bottom_pane.show_selection_view(SelectionViewParams {
            title: Some("Managed worktrees".to_string()),
            subtitle: Some(
                if entries.is_empty() {
                    "No worktrees in this repository's configured pool"
                } else {
                    "Select a worktree to resume its owner or copy its working directory"
                }
                .to_string(),
            ),
            is_searchable: true,
            items: entries
                .into_iter()
                .map(|entry| {
                    let request = request.clone();
                    SelectionItem {
                        name: entry.cwd.display().to_string(),
                        search_value: Some(entry.cwd.display().to_string()),
                        description: Some(entry.owner.map_or_else(
                            || "No owner metadata".to_string(),
                            |owner| format!("Owner: {owner}"),
                        )),
                        actions: vec![Box::new(move |tx| {
                            tx.send(AppEvent::ShowManagedWorktreeActions {
                                request: request.clone(),
                                entry: entry.clone(),
                            })
                        })],
                        dismiss_on_select: true,
                        ..Default::default()
                    }
                })
                .collect(),
            footer_hint: Some(standard_popup_hint_line()),
            ..Default::default()
        });
    }

    pub(crate) fn managed_worktree_action(
        &self,
        request: &Request,
        action: Action,
    ) -> Option<AppEvent> {
        if !self.worktree_request_is_current(request) {
            return None;
        }
        Some(match action {
            Action::Resume(owner) => AppEvent::ResumeSessionByIdOrName(owner.to_string()),
            Action::Copy(cwd) => AppEvent::CopySelection {
                text: cwd.to_str()?.into(),
                label: "Worktree working directory".to_string(),
                format: crate::clipboard_copy::CopyFormat::PlainText,
            },
        })
    }

    pub(crate) fn show_managed_worktree_actions(&mut self, request: Request, entry: Entry) {
        if !self.worktree_request_is_current(&request) {
            return;
        }
        let mut items = Vec::new();
        if let Some(owner) = entry.owner {
            let request = request.clone();
            items.push(SelectionItem {
                name: "Resume owner thread".to_string(),
                actions: vec![Box::new(move |tx| {
                    tx.send(AppEvent::ManagedWorktreeAction {
                        request: request.clone(),
                        action: Action::Resume(owner),
                    })
                })],
                dismiss_on_select: true,
                ..Default::default()
            });
        }
        let cwd = entry.cwd.clone();
        items.push(SelectionItem {
            name: "Copy working directory".to_string(),
            is_disabled: entry.cwd.to_str().is_none(),
            disabled_reason: entry
                .cwd
                .to_str()
                .is_none()
                .then(|| "Path is not valid UTF-8".to_string()),
            actions: vec![Box::new(move |tx| {
                tx.send(AppEvent::ManagedWorktreeAction {
                    request: request.clone(),
                    action: Action::Copy(cwd.clone()),
                })
            })],
            dismiss_on_select: true,
            ..Default::default()
        });
        self.bottom_pane.show_selection_view(SelectionViewParams {
            title: Some("Worktree".to_string()),
            subtitle: Some(entry.cwd.display().to_string()),
            items,
            footer_hint: Some(standard_popup_hint_line()),
            ..Default::default()
        });
    }
}
