//! Settings > Ely.by: credentials never leave the masked fields except for the auth request.
use super::*;
use servers_page::Field;

impl Launcher {
    pub(super) fn start_elyby(&mut self) {
        if self.elyby_job.is_some() {
            return;
        }
        let login = self.servers.value(Field::ElyLogin).to_owned();
        let password = std::mem::take(self.servers.text(Field::ElyPassword));
        let code = std::mem::take(self.servers.text(Field::ElyCode));
        let (tx, rx) = channel();
        self.elyby_job = Some(rx);
        self.elyby_status = None;
        std::thread::spawn(move || {
            let result = account::elyby::sign_in(&login, &password, &code).map(|a| {
                format!(
                    "Signed in to Ely.by as {}. Ely.by online mode selected.",
                    a.name
                )
            });
            let _ = tx.send(result);
        });
    }

    pub(super) fn elyby_tick(&mut self) -> bool {
        let Some(rx) = &self.elyby_job else {
            return false;
        };
        let result = match rx.try_recv() {
            Ok(result) => result,
            Err(std::sync::mpsc::TryRecvError::Empty) => return false,
            Err(_) => Err("The Ely.by request stopped. Try again.".into()),
        };
        self.elyby_job = None;
        self.elyby_status = Some(result);
        self.accounts = Accounts::load();
        self.owner_screen_open = false;
        true
    }

    pub(super) fn elyby_rows(&mut self, mut n: usize, cx: &mut Context<Self>) -> (Div, usize) {
        let t = self.theme;
        let mut rows = div().flex().flex_col();
        let busy = self.elyby_job.is_some();
        for (i, a) in self.accounts.elyby.clone().into_iter().enumerate() {
            let uuid = a.uuid.clone();
            let selected = self.accounts.elyby_selected.as_ref() == Some(&uuid);
            let pick = uuid.clone();
            let select = button(
                format!("elyby-select-{i}"),
                if selected { "Selected" } else { "Play as" },
                Kind::Secondary,
                &t,
            )
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.play_as(account::Player::ElyBy(pick.clone()));
                cx.notify();
            }));
            let logout = button(format!("elyby-logout-{i}"), "Sign out", Kind::Ghost, &t).on_click(
                cx.listener(move |this, _: &ClickEvent, _, cx| {
                    if this.elyby_job.is_some() {
                        return;
                    }
                    let (tx, rx) = channel();
                    this.elyby_job = Some(rx);
                    let uuid = uuid.clone();
                    std::thread::spawn(move || {
                        let _ = tx.send(
                            account::elyby::sign_out(&uuid)
                                .map(|()| "Signed out of Ely.by on this device.".into()),
                        );
                    });
                    cx.notify();
                }),
            );
            n += 1;
            rows = rows.child(index_row(
                n,
                a.name,
                Some(div().child("Ely.by · online").into_any_element()),
                Some(
                    div()
                        .flex()
                        .gap(px(6.))
                        .child(if busy { disabled(select) } else { select })
                        .child(if busy { disabled(logout) } else { logout })
                        .into_any_element(),
                ),
                &t,
            ));
        }
        let login = self.field(Field::ElyLogin, "Ely.by username or email", 300., cx);
        let password = self.field(Field::ElyPassword, "Password", 300., cx);
        let code = self.field(Field::ElyCode, "2FA code (if enabled)", 300., cx);
        let sign_in = button(
            "elyby-sign-in",
            if busy {
                "Connecting…"
            } else {
                "Sign in with Ely.by"
            },
            Kind::Secondary,
            &t,
        )
        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
            this.start_elyby();
            cx.notify();
        }));
        let mut form = div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(login)
            .child(password)
            .child(code)
            .child(if busy { disabled(sign_in) } else { sign_in });
        if let Some(status) = &self.elyby_status {
            form = form.child(
                div()
                    .max_w(px(320.))
                    .text_color(if status.is_err() { t.danger } else { t.ink })
                    .child(match status {
                        Ok(s) | Err(s) => s.clone(),
                    }),
            );
        }
        if !self.keychain {
            form = form.child(div().max_w(px(320.)).child(account::NO_KEYCHAIN));
        }
        n += 1;
        rows = rows.child(index_row(
            n,
            "Ely.by",
            Some(
                div()
                    .max_w(px(400.))
                    .child(account::elyby::ONLINE_INFO)
                    .into_any_element(),
            ),
            Some(form.into_any_element()),
            &t,
        ));
        (rows, n)
    }
}
