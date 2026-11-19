use eframe::egui::{self, RichText, TextEdit};
use imdev::core::Request;
use imdev::policy::{self, AuthMode, Exclusions, RequestDraft, Snapshot};
use crate::theme::{self, Role};
#[cfg(test)]
#[path="../tests/support/crypto.rs"]
mod test_crypto;

pub struct ImdevApp {
    request: Request,
    query: Vec<(String,String)>,
    headers: Vec<(String,String)>,
    request_tab: usize,
    response_tab: usize,
    notice: String,
    busy: bool,
    runtime: Option<tokio::runtime::Runtime>,
    running: Option<Running>,
    response: Option<imdev::core::ResponseData>,
    displayed_body: String,
    store: Option<imdev::repository::Repository>,
    storage_warning: String,
    modal: Option<Modal>,
    save_name: String,
    auth_settings: bool,
    auth_origin: String,
    auth_token: String,
    session: imdev::policy::AuthSession,
    auth: AuthMode,
    exclusions: Exclusions,
    exclusion_text: [String;4],
    baseline: RequestDraft,
    missing: Vec<String>,
    reviewed: bool,
}
#[derive(Clone)]
enum Modal { Save(Snapshot), Delete(String), ClearHistory, Replace(Snapshot) }
struct Running {
    snapshot: Snapshot,
    cancel: tokio::sync::watch::Sender<bool>,
    result: std::sync::mpsc::Receiver<Result<imdev::core::ResponseData, String>>,
    task: tokio::task::JoinHandle<()>,
}
impl ImdevApp {
    pub fn new(ctx: &egui::Context) -> Self { Self::with_path(ctx, imdev::repository::Repository::default_path()) }
    fn memory(ctx: &egui::Context) -> Self {
        theme::apply(ctx);
        theme::load_korean_font(ctx);
        Self { request: Request::default(), query: vec![], headers: vec![], request_tab: 0, response_tab: 0, notice: String::new(), busy: false, runtime: tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().ok(), running: None, response: None, displayed_body: String::new(), store: None, storage_warning: String::new(), modal: None, save_name: String::new(), auth_settings:false, auth_origin:String::new(), auth_token:String::new(), session:Default::default(),auth:Default::default(),exclusions:Default::default(),exclusion_text:Default::default(),baseline:Default::default(),missing:vec![],reviewed:true }
    }
    fn with_path(ctx: &egui::Context, path: std::path::PathBuf) -> Self {
        let mut app = Self::memory(ctx);
        #[cfg(test)]
        let opened=imdev::repository::Repository::open_with(path,test_crypto::protector());
        #[cfg(not(test))]
        let opened=imdev::repository::Repository::open(path);
        match opened {
            Ok(store) => {app.session.origin=store.auth_origin.clone();app.store = Some(store);},
            Err(e) => app.storage_warning = format!("Storage could not be opened. Saving and history are disabled; original file was not replaced. {e}"),
        }
        #[cfg(not(windows))]
        {app.storage_warning.push_str(" Windows DPAPI encryption is unavailable on this platform; persistence fails closed, with no plaintext fallback.");}
        app
    }
    fn open_save(&mut self) -> bool {
        if self.busy || self.store.is_none() { return false; }
        match self.draft().and_then(|draft|policy::snapshot(&draft,&self.session)) {
            Ok(request) => { self.modal = Some(Modal::Save(request)); self.save_name.clear(); self.notice.clear(); true }
            Err(e) => { self.notice = e; false }
        }
    }
    fn apply_auth(&mut self)->bool {
        let origin=match policy::canonical_origin(&self.auth_origin){Ok(origin)=>origin,Err(e)=>{self.notice=e;return false;}};
        if self.auth_token.starts_with("Bearer ") || self.auth_token.chars().any(|c|c.is_control()) {self.notice="Paste a bare Bearer token without prefix or control characters.".into();return false;}
        let persisted=self.store.as_mut().ok_or_else(||"Encrypted storage unavailable".to_string()).and_then(|store|store.set_auth_origin(&origin));
        self.session.origin=origin;self.session.token=std::mem::take(&mut self.auth_token);
        self.notice=match persisted {Ok(())=>"Common origin encrypted. Token is in session memory only; restart requires re-entry.".into(),Err(e)=>format!("Applied for this session only. Origin was NOT persisted: {e}. No plaintext fallback.")};
        self.auth_settings=false;true
    }
    fn confirm_modal(&mut self) -> bool {
        let Some(modal) = self.modal.clone() else { return false; };
        if let Modal::Replace(snapshot)=modal {self.modal=None;self.install(snapshot);return true;}
        if matches!(modal,Modal::Save(_)) && !self.session.token.is_empty() && self.save_name.contains(&self.session.token) {self.notice="Template name contains the session token. Never put secrets in names.".into();return false;}
        let Some(store) = self.store.as_mut() else { return false; };
        let result = match modal {
            Modal::Save(request) => store.save(self.save_name.trim(), &request),
            Modal::Delete(id) => store.remove(&id),
            Modal::ClearHistory => store.clear_history(),
            Modal::Replace(_) => unreachable!(),
        };
        match result {
            Ok(()) => { self.modal = None; self.notice = "Encrypted library updated. General request data retained; excluded values need re-entry. Tokens stay in session memory only.".into(); true }
            Err(e) => { self.notice = e; false }
        }
    }
    fn start(&mut self, ctx: &egui::Context) -> bool {
        if self.busy || self.modal.is_some() || self.auth_settings { return false; }
        if !self.reviewed {self.notice="Review the restored exclusions and acknowledge before Send.".into();return false;}
        let (request,snapshot) = match self.draft().and_then(|draft| Ok((policy::prepare(&draft,&self.session)?,policy::snapshot(&draft,&self.session)?))) {
            Ok(r) => r, Err(e) => { self.notice = e; return false; }
        };
        let Some(runtime) = &self.runtime else { self.notice = "Async runtime unavailable; sending disabled".into(); return false; };
        let (cancel, receiver) = tokio::sync::watch::channel(false);
        let (sender, result) = std::sync::mpsc::channel();
        let worker_request = request.clone();
        let repaint = ctx.clone();
        let task = runtime.spawn(async move {
            let outcome = imdev::core::execute(worker_request, receiver).await;
            let _ = sender.send(outcome);
            repaint.request_repaint();
        });
        self.running = Some(Running { snapshot, cancel, result, task });
        self.busy = true;
        self.response = None;
        self.displayed_body.clear();
        self.notice.clear();
        true
    }
    fn poll_result(&mut self) {
        let result = self.running.as_ref().and_then(|running| match running.result.try_recv() {
            Ok(result) => Some(result),
            Err(std::sync::mpsc::TryRecvError::Disconnected) => Some(Err("Request worker stopped".into())),
            Err(std::sync::mpsc::TryRecvError::Empty) => None,
        });
        if let Some(result) = result {
            let running = self.running.take().expect("result has running owner");
            let disk_error = self.store.as_mut().and_then(|store| store.record(&running.snapshot, &result).err());
            self.busy = false;
            match result {
                Ok(response) => { self.displayed_body = response.body.clone(); self.response = Some(response); self.notice = "Response received; response content stays in memory only".into(); }
                Err(e) => { self.notice = e; }
            }
            if disk_error.is_some() { self.storage_warning = "Request completed, but history could not be persisted. Nothing was retried.".into(); }
        }
    }
    fn load_legacy(&mut self, request: Request) -> bool {
        let loaded=self.restore(Snapshot{draft:RequestDraft{request,auth:AuthMode::None,exclusions:Default::default()},missing:vec!["Legacy origin-only record: path, query/header values and body are irrecoverable. Partial load only; re-enter and review before Send.".into()]});
        if loaded {self.notice="Legacy partial load only: discarded path, query/header values and body are irrecoverable. Auth is None; nothing was sent.".into();}loaded
    }
    fn new_request(&mut self)->bool {self.restore(Snapshot{draft:Default::default(),missing:vec![]})}
    #[cfg(test)]
    fn load_request(&mut self, request: Request) -> bool {
        self.restore(Snapshot{draft:RequestDraft{request,auth:AuthMode::None,exclusions:Default::default()},missing:vec![]})
    }
    fn draft(&self)->Result<RequestDraft,String>{
        if self.exclusion_text[3].split(',').map(str::trim).filter(|v|!v.is_empty()).any(|v|v.parse::<usize>().ok().filter(|n|*n>0).is_none()){return Err("Path segment indices must be positive integers.".into());}
        Ok(RequestDraft{request:self.request_snapshot()?,auth:self.auth.clone(),exclusions:self.exclusions.clone()})
    }
    fn restore(&mut self,snapshot:Snapshot)->bool {
        if self.busy { return false; }
        if self.draft().map_or(true,|d|d!=self.baseline) {self.modal=Some(Modal::Replace(snapshot));return false;}
        self.install(snapshot);true
    }
    fn install(&mut self,snapshot:Snapshot) {
        self.baseline=snapshot.draft.clone();self.auth=snapshot.draft.auth;self.exclusions=snapshot.draft.exclusions;
        self.exclusion_text=[self.exclusions.query_keys.join(", "),self.exclusions.header_names.join(", "),self.exclusions.json_pointers.join(", "),self.exclusions.path_segments.iter().map(|n|n.to_string()).collect::<Vec<_>>().join(", ")];
        self.missing=snapshot.missing;self.reviewed=self.missing.is_empty();
        let request=snapshot.draft.request;
        self.query = request.query.clone();
        self.headers = request.headers.clone();
        self.request = request;
        self.response=None;
        self.displayed_body.clear();
        self.response_tab=0;
        self.notice="Restored into editor only. Review and select Send explicitly; no request has been sent.".into();
    }
    fn request_snapshot(&self) -> Result<Request, String> {
        let mut request = self.request.clone();
        request.query = pair_snapshot(&self.query,"Query")?;
        request.headers = pair_snapshot(&self.headers,"Headers")?;

        Ok(request)
    }
    fn command_bar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            ui.add_enabled_ui(!self.busy, |ui| { egui::ComboBox::from_id_salt("method").width(88.0).selected_text(RichText::new(&self.request.method).font(theme::font(Role::Url)).color(theme::SUCCESS)).show_ui(ui, |ui| {
                for method in ["GET","POST","PUT","PATCH","DELETE","HEAD","OPTIONS"] { ui.selectable_value(&mut self.request.method,method.into(),RichText::new(method).font(theme::font(Role::Url))); }
            }); });
            let send_width=if self.busy {112.0} else {88.0};
            ui.add_enabled(!self.busy,TextEdit::singleline(&mut self.request.url).font(theme::font(Role::Url)).margin(egui::vec2(10.0,9.0)).hint_text("https://example.com/api").desired_width((ui.available_width()-send_width-32.0).max(60.0)));
            if self.busy {
                let cancelling=self.running.as_ref().is_some_and(|r|*r.cancel.borrow());
                if ui.add_enabled(!cancelling,egui::Button::new(if cancelling {"Cancelling…"} else {"Cancel"}).min_size(egui::vec2(send_width,34.0))).clicked() { if let Some(r)=&self.running { let _=r.cancel.send(true); } }
            } else if ui.add_enabled(self.runtime.is_some(),egui::Button::new(RichText::new("Send").strong().color(theme::ACCENT_INK)).fill(theme::ACCENT).min_size(egui::vec2(send_width,34.0))).clicked() { self.start(ctx); }
        });
        ui.add_space(4.0);
        theme::control_row(ui,|ui| {
            ui.label(RichText::new("Timeout (ms)").font(theme::font(Role::Caption)).color(theme::MUTED));
            theme::timeout_control(ui,&mut self.request.timeout_ms,!self.busy);
            if theme::row_action(ui,"Save template",Role::Caption,!self.busy && self.store.is_some()).clicked() {self.open_save();}
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui| {
                ui.add_enabled_ui(!self.busy,|ui|{egui::ComboBox::from_id_salt("request_auth").width(86.0).selected_text(format!("{:?}",self.auth)).show_ui(ui,|ui|{for mode in [AuthMode::Auto,AuthMode::None,AuthMode::Common]{let text=format!("{mode:?}");ui.selectable_value(&mut self.auth,mode,text);}});});
                ui.label(RichText::new("Auth").font(theme::font(Role::Caption)).color(theme::MUTED)).on_hover_text("Auto: common auth for exact origin only. None: no shared auth. Send: Ctrl / Cmd + Enter.");
            });
        });
    }
    fn request_card(&mut self, ui:&mut egui::Ui, size:egui::Vec2) {
        let card=theme::card().show(ui,|ui| {
            ui.set_min_size(size-egui::vec2(30.0,30.0)); ui.set_max_size(size-egui::vec2(30.0,30.0));
            ui.label(RichText::new("요청 · Request editor").font(theme::font(Role::Section)).strong());
            ui.add_space(8.0);
            ui.horizontal(|ui|tabs(ui,&mut self.request_tab,&["Query","Headers","Body","Exclusions"]));
            ui.add_space(8.0);
            ui.add_enabled_ui(!self.busy,|ui| {
                if self.request_tab==2 { ui.horizontal(|ui| {for kind in ["none","json","text"] {ui.selectable_value(&mut self.request.body_type,kind.into(),kind);}}); }
                let h=(ui.available_height()-38.0).max(40.0);
                egui::ScrollArea::vertical().id_salt("request_editor_scroll").max_height(h).auto_shrink([false,false]).show(ui,|ui| {
                    if self.request_tab==3 {
                        ui.label("Arbitrary secret detection is not complete. Exclude sensitive values before saving or sending.");
                        exclusion_list(ui,"Query keys",&mut self.exclusions.query_keys,&mut self.exclusion_text[0]);
                        exclusion_list(ui,"Header names",&mut self.exclusions.header_names,&mut self.exclusion_text[1]);
                        exclusion_list(ui,"JSON pointers",&mut self.exclusions.json_pointers,&mut self.exclusion_text[2]);
                        ui.label("Path segments · 1-based, comma-separated");
                        if ui.text_edit_singleline(&mut self.exclusion_text[3]).changed(){match self.exclusion_text[3].split(',').map(str::trim).filter(|v|!v.is_empty()).map(|v|v.parse::<usize>().ok().filter(|n|*n>0)).collect::<Option<Vec<_>>>(){Some(values)=>self.exclusions.path_segments=values,None=>self.notice="Path segment indices must be positive integers.".into()}}
                        ui.checkbox(&mut self.exclusions.body,"Exclude full body");
                    } else if self.request_tab<2 {
                        let rows=if self.request_tab==0 {&mut self.query}else{&mut self.headers};
                        let exclusions=if self.request_tab==0 {&self.exclusions.query_keys}else{&self.exclusions.header_names};
                        pair_table(ui,rows,self.request_tab,exclusions,&self.session);
                    } else {
                        ui.add_enabled(self.request.body_type!="none",TextEdit::multiline(&mut self.request.body).font(theme::font(Role::Mono)).hint_text("Request body").margin(10.0).desired_width(f32::INFINITY).desired_rows(1).min_size(egui::vec2(0.0,h-4.0)));
                    }
                });
            });
            ui.label(RichText::new(if self.request_tab==2 {"Body is sent only for json or text."} else if self.request_tab==3 {"Exclusions apply to persisted snapshots, not the live draft."} else {"Ordered rows · Duplicate keys and empty values preserved"}).font(theme::font(Role::Small)).color(theme::MUTED));
        });
        #[cfg(test)]
        ui.ctx().data_mut(|d|d.insert_temp(egui::Id::new("request_actual"),card.response.rect));
        let _=card;
    }
    fn response_card(&mut self,ui:&mut egui::Ui,ctx:&egui::Context,size:egui::Vec2) {
        let card=theme::card().show(ui,|ui| {
            ui.set_min_size(size-egui::vec2(30.0,30.0)); ui.set_max_size(size-egui::vec2(30.0,30.0));
            ui.horizontal(|ui| {
                ui.label(RichText::new("응답 · Response").font(theme::font(Role::Section)).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui| {ui.label(RichText::new(if self.busy {"IN FLIGHT"} else if self.response.is_some() {"RECEIVED"} else {"READY"}).font(theme::font(Role::Small)).color(theme::MUTED));});
            });
            ui.add_space(8.0);
            theme::control_row(ui,|ui| {
                tabs(ui,&mut self.response_tab,&["Body","Headers"]);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui| {
                    if theme::row_action(ui,"Copy tab",Role::Caption,self.response.is_some()).clicked() {
                        ctx.copy_text(if self.response_tab==0 {self.displayed_body.clone()} else {pairs_text(&self.response.as_ref().unwrap().headers)}); self.notice="Selected response tab copied to the system clipboard.".into();
                    }
                    if theme::row_action(ui,"Pretty JSON",Role::Caption,self.response.is_some()).clicked() {match pretty_json(&self.response.as_ref().unwrap().body) {Ok(body)=>{self.displayed_body=body;self.response_tab=0;},Err(e)=>self.notice=e}}
                });
            });
            ui.add_space(10.0);
            egui::ScrollArea::vertical().id_salt("response_content").max_height(ui.available_height()).auto_shrink([false,false]).show(ui,|ui| {
                if self.busy { ui.add_space(16.0);ui.horizontal(|ui| {ui.spinner();ui.label("Sending request…");}); ui.label(RichText::new("Waiting for the server. Cancel stops this request.").font(theme::font(Role::Caption)).color(theme::MUTED)); }
                else if let Some(response)=&self.response {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(format!("{} {}",response.status,response.status_text)).font(theme::font(Role::Sidebar)).strong().color(if response.status>=400 {theme::ERROR} else {theme::SUCCESS}));
                        ui.label(RichText::new(format!("{} ms  ·  {} B",response.elapsed_ms,response.body.len())).font(theme::font(Role::Caption)).color(theme::MUTED));
                    });
                    if response.truncated {ui.colored_label(theme::ERROR,"Response truncated at the 2 MiB limit");}
                    ui.add_space(8.0);
                    let mut text=if self.response_tab==0 {self.displayed_body.clone()} else {pairs_text(&response.headers)};
                    ui.add(TextEdit::multiline(&mut text).font(theme::font(Role::Mono)).margin(10.0).desired_width(f32::INFINITY).desired_rows(8));
                } else {
                    ui.add_space(if self.notice.starts_with("Restored") {8.0}else{24.0});
                    ui.label(RichText::new(if self.notice.starts_with("Restored") {"Restored · not sent"} else if self.notice.is_empty() || self.notice.starts_with("New request") {"Waiting for your first request"} else {"No response"}).font(theme::font(Role::Body)).strong());
                    ui.add_space(6.0);
                    ui.label(RichText::new(if self.notice.starts_with("Restored") {"Review the editor, then select Send explicitly.\nRestoring history or templates never sends HTTP."}else{"Enter an endpoint and select Send.\nResponse body, headers and timing appear here."}).font(theme::font(Role::Caption)).color(theme::MUTED));
                }
                if !self.missing.is_empty(){
                    ui.add_space(10.0);ui.colored_label(theme::ERROR,"Sensitive values omitted · re-entry required");
                    for location in &self.missing {ui.label(RichText::new(location).font(theme::font(Role::Caption)));}
                    ui.checkbox(&mut self.reviewed,"I reviewed the restored exclusions");
                    ui.label("Markers always block Send until replaced or removed. Acknowledgement does not bypass re-entry.");
                }
                if !self.notice.is_empty() {ui.add_space(12.0);ui.label(RichText::new(&self.notice).font(theme::font(Role::Caption)).color(if !self.busy && self.response.is_none() && !self.notice.starts_with("New request") {theme::ERROR} else {theme::MUTED}));}
            });
        });
        #[cfg(test)]
        ui.ctx().data_mut(|d|d.insert_temp(egui::Id::new("response_actual"),card.response.rect));
        let _=card;
    }
    fn inspectors(&mut self,ui:&mut egui::Ui,ctx:&egui::Context,area:egui::Rect,wide:bool) {
        let gap=14.0;
        let (request_rect,response_rect)=if wide {
            let w=(area.width()-gap)*0.5;
            (egui::Rect::from_min_size(area.min,egui::vec2(w,area.height())),egui::Rect::from_min_size(area.min+egui::vec2(w+gap,0.0),egui::vec2(w,area.height())))
        }else{
            let h=(area.height()-gap)*0.5;
            (egui::Rect::from_min_size(area.min,egui::vec2(area.width(),h)),egui::Rect::from_min_size(area.min+egui::vec2(0.0,h+gap),egui::vec2(area.width(),h)))
        };
        #[cfg(test)]
        ctx.data_mut(|d|d.insert_temp(egui::Id::new("proposal_panels"),(request_rect,response_rect)));
        ui.scope_builder(egui::UiBuilder::new().max_rect(request_rect),|ui|self.request_card(ui,request_rect.size()));
        ui.scope_builder(egui::UiBuilder::new().max_rect(response_rect),|ui|self.response_card(ui,ctx,response_rect.size()));
        ui.advance_cursor_after_rect(area);
    }
    pub fn show(&mut self, ctx: &egui::Context) {
        theme::apply(ctx);
        self.poll_result();

        if self.modal.is_none() && !self.auth_settings {
            let send = ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Enter) || i.consume_key(egui::Modifiers::CTRL, egui::Key::Enter));
            if send { self.start(ctx); }
            if !ctx.wants_keyboard_input() && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::N)) { self.new_request(); }
        }
        egui::TopBottomPanel::bottom("privacy").frame(egui::Frame::new().fill(theme::BG).inner_margin(egui::Margin::symmetric(18, 9))).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("LOCAL STORAGE").font(theme::font(Role::Small)).color(theme::MUTED));
                ui.label(RichText::new("General requests encrypted with Windows DPAPI · Auth token in session memory only").font(theme::font(Role::Caption)).color(theme::MUTED));
            });
            if !self.storage_warning.is_empty() { ui.colored_label(theme::ERROR, &self.storage_warning); }
            if self.runtime.is_none() { ui.colored_label(theme::ERROR, "Async runtime unavailable; sending disabled"); }
        });
        egui::SidePanel::left("library").exact_width(220.0).resizable(false)
            .frame(egui::Frame::new().fill(theme::SIDEBAR).inner_margin(16)).show(ctx, |ui| {
            ui.add_space(8.0);
            ui.label(RichText::new("Imdev").font(theme::font(Role::Brand)).strong());
            ui.label(RichText::new("LOCAL API WORKBENCH").font(theme::font(Role::Small)).color(theme::MUTED));
            ui.add_space(22.0);
            if ui.add_enabled(!self.busy, egui::Button::new("+  New request").min_size(egui::vec2(ui.available_width(),34.0))).clicked() { self.new_request(); }
            if ui.add_enabled(!self.busy,egui::Button::new("Common auth settings")).clicked(){self.auth_origin=self.session.origin.clone();self.auth_token=self.session.token.clone();self.auth_settings=true;}
            ui.add_space(12.0);
            let (legacy_saved,legacy_history)=self.store.as_ref().map(|s|(s.legacy_saved.clone(),s.legacy_history.clone())).unwrap_or_default();
            if !legacy_saved.is_empty() || !legacy_history.is_empty(){
                egui::CollapsingHeader::new("Legacy · read-only").show(ui,|ui|{
                    ui.label(RichText::new("Origin only. Discarded path, query/header values and body are irrecoverable.").font(theme::font(Role::Small)).color(theme::MUTED));
                    egui::ScrollArea::vertical().id_salt("legacy_list").max_height(100.0).show(ui,|ui|{
                        for entry in legacy_saved {if ui.add_enabled(!self.busy,egui::Button::new(format!("{} · partial load",entry.name)).wrap()).clicked(){self.load_legacy(entry.request);}}
                        for entry in legacy_history {if ui.add_enabled(!self.busy,egui::Button::new(format!("{} {} · partial",entry.method,entry.url)).wrap()).clicked(){self.load_legacy(Request{method:entry.method,url:entry.url,..Default::default()});}}
                    });
                });
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| { ui.label(RichText::new("Saved templates").font(theme::font(Role::Section)).strong()); });
            let saved=self.store.as_ref().map(|s|s.saved.clone()).unwrap_or_default();
            let section_height=((ui.available_height()-80.0)*0.44).max(90.0);
            egui::ScrollArea::vertical().id_salt("saved_list").max_height(section_height).min_scrolled_height(section_height).auto_shrink([false,false]).show(ui,|ui| {
                if saved.is_empty() { ui.add_space(12.0); ui.label(RichText::new("No saved templates").font(theme::font(Role::Sidebar)).color(theme::MUTED)); ui.label(RichText::new("Save a reusable, redacted request from the editor.").font(theme::font(Role::Caption)).color(theme::MUTED)); }
                for entry in saved {
                    ui.push_id(&entry.id,|ui| {
                        let label=RichText::new(format!("{}  {}",entry.snapshot.draft.request.method,entry.name)).font(theme::font(Role::Sidebar));
                        if ui.add_enabled(!self.busy,egui::Button::new(label).wrap().min_size(egui::vec2(ui.available_width(),30.0))).clicked() {
                            self.restore(entry.snapshot.clone());
                        }
                        ui.label(RichText::new(&entry.snapshot.draft.request.url).font(theme::font(Role::Small)).color(theme::MUTED));
                        if theme::row_action(ui,"Delete",Role::Caption,!self.busy).clicked() { self.modal=Some(Modal::Delete(entry.id.clone())); }
                        ui.add_space(6.0);
                    });
                }
            });
            ui.separator();
            theme::control_row(ui,|ui| { ui.label(RichText::new("History").font(theme::font(Role::Section)).strong()); if theme::row_action(ui,"Clear history",Role::Section,!self.busy && self.store.is_some()).clicked() { self.modal=Some(Modal::ClearHistory); } });
            ui.label(RichText::new("Clears new history only · Legacy retained").font(theme::font(Role::Small)).color(theme::MUTED));
            let history=self.store.as_ref().map(|s|s.history.clone()).unwrap_or_default();
            egui::ScrollArea::vertical().id_salt("history_list").max_height(ui.available_height()).auto_shrink([false,false]).show(ui,|ui| {
                if history.is_empty() { ui.add_space(12.0); ui.label(RichText::new("No requests yet").font(theme::font(Role::Sidebar)).color(theme::MUTED)); ui.label(RichText::new("Completed requests can be restored into the editor without sending.").font(theme::font(Role::Caption)).color(theme::MUTED)); }
                for entry in history { ui.push_id(&entry.id,|ui| {
                    let history_button=ui.add_enabled(!self.busy,egui::Button::new(RichText::new(format!("{} {}",entry.snapshot.draft.request.method,entry.snapshot.draft.request.url)).font(theme::font(Role::Sidebar))).wrap());

                    if history_button.clicked() {
                        self.restore(entry.snapshot.clone());
                    }
                    ui.label(RichText::new(format!("{} · {} ms · {}",if entry.error {"error".into()} else {entry.status.map(|s|s.to_string()).unwrap_or_default()},entry.elapsed_ms,history_age(entry.at,std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs()))).font(theme::font(Role::Caption)).color(theme::MUTED));
                    ui.add_space(6.0);
                }); }
            });
        });
        egui::CentralPanel::default().frame(egui::Frame::new().fill(theme::BG).inner_margin(20)).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Workspace  /  Untitled request").font(theme::font(Role::Caption)).color(theme::MUTED));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| { ui.label(RichText::new("DESIGN PROPOSAL").font(theme::font(Role::Small)).color(theme::MUTED)); });
            });
            ui.add_space(16.0);
            ui.label(RichText::new("요청 · Request workspace").font(theme::font(Role::Headline)).strong());
            ui.add_space(14.0);
            self.command_bar(ui,ctx);
            ui.add_space(14.0);
            let area=ui.available_rect_before_wrap();
            let wide=ctx.content_rect().width()>1050.0;
            if !wide && area.height()<400.0 {
                let scroll=egui::ScrollArea::vertical().id_salt("small_window_inspectors").auto_shrink([false,false]).max_height(area.height()).show(ui,|ui| {
                    let content=egui::Rect::from_min_size(ui.next_widget_position(),egui::vec2(ui.available_width(),480.0));
                    self.inspectors(ui,ctx,content,false);
                });
                #[cfg(test)]
                ctx.data_mut(|d|{d.insert_temp(egui::Id::new("inspectors_viewport"),scroll.inner_rect);d.insert_temp(egui::Id::new("inspectors_scroll_offset"),scroll.state.offset.y);});
                let _=scroll;
            }else{self.inspectors(ui,ctx,area,wide);}
        });
        if self.auth_settings {
            egui::Modal::new(egui::Id::new("common_auth_settings")).show(ctx,|ui| {
                ui.set_width(440.0);
                ui.label(RichText::new("Common authentication").font(theme::font(Role::Headline)));
                ui.label("Exact HTTP(S) origin · scheme, host and port");
                ui.add(TextEdit::singleline(&mut self.auth_origin).desired_width(f32::INFINITY).hint_text("https://api.example.com"));
                ui.add_space(10.0); ui.label("Bearer token · paste the bare token, without Bearer");
                ui.add(TextEdit::singleline(&mut self.auth_token).password(true).desired_width(f32::INFINITY));
                ui.add_space(10.0); ui.label("Token stays in session memory only. Restart requires re-entry.");
                ui.label("Only origin metadata can be encrypted on supported platforms. No plaintext fallback.");
                if !self.notice.is_empty(){ui.colored_label(theme::ERROR,&self.notice);}
                ui.horizontal(|ui|{
                    if ui.button("Cancel").clicked(){self.auth_settings=false;self.auth_token.clear();}
                    if ui.button("Apply").clicked(){self.apply_auth();}
                });
            });
        }
        if let Some(modal)=self.modal.clone() {
            let result=egui::Modal::new(egui::Id::new("library_confirmation")).show(ctx,|ui| {
                ui.set_max_width(460.0);
                ui.label(RichText::new(match &modal {Modal::Save(_)=>"Save safe template?",Modal::Delete(_)=>"Delete template?",Modal::ClearHistory=>"Clear new history?",Modal::Replace(_)=>"Replace edited request?"}).font(theme::font(Role::Headline)));
                match &modal {
                    Modal::Save(request)=>{
                        ui.label("General request data is encrypted. Common auth is stored as a reference, never as a session token. Excluded fields need re-entry.");
                        ui.label("Do not put secrets in template names or field names. Arbitrary secret detection is not complete; review exclusions before saving.");
                        ui.label(RichText::new(&request.draft.request.url).font(theme::font(Role::Url)));
                        ui.label("Template name"); ui.add(TextEdit::singleline(&mut self.save_name).font(theme::font(Role::Body)));
                    }
                    Modal::Replace(_)=>{ui.label("Unsaved editor changes will be replaced. Cancel keeps your draft. Confirm only loads the editor and never sends HTTP.");}
                    Modal::Delete(_)=>{ui.label("This permanently removes the selected saved template.");}
                    Modal::ClearHistory=>{ui.label("Remove all recent request history? Saved templates are kept.");}
                }
                if !self.notice.is_empty(){ui.label(&self.notice);}
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked(){self.modal=None;}
                    if ui.button("Confirm").clicked(){self.confirm_modal();}
                });
            });
            if result.should_close(){self.modal=None;}
        }
    }
}
// Reuse the existing policy's recognition (including encoded token copies), not
// a second security-policy list. The probe is memory-only and never sent/saved.
fn pair_value_masked(key:&str,value:&str,exclusions:&[String],session:&policy::AuthSession)->bool {
    let draft=RequestDraft{request:Request{url:"http://127.0.0.1/".into(),query:vec![(key.into(),value.into())],..Default::default()},auth:AuthMode::None,exclusions:Exclusions{query_keys:exclusions.to_vec(),..Default::default()}};
    policy::snapshot(&draft,session).map_or(true,|snapshot|snapshot.draft.request.query[0].1==policy::REDACTED)
}
fn pair_table(ui:&mut egui::Ui,rows:&mut Vec<(String,String)>,tab:usize,exclusions:&[String],session:&policy::AuthSession) {
    let gap=6.0;
    let width=ui.available_width();
    let key_width=((width-136.0)*0.38).max(60.0);
    let value_width=(width-key_width-136.0).max(60.0);
    let caption=|text:&str|RichText::new(text).font(theme::font(Role::Caption));
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x=gap;
        ui.add_sized([key_width,28.0],egui::Label::new(caption("Key").color(theme::MUTED)));
        ui.add_sized([value_width,28.0],egui::Label::new(caption("Value").color(theme::MUTED)));
        if ui.add(egui::Button::new(caption("Add row")).min_size(egui::vec2(88.0,28.0))).clicked(){rows.push((String::new(),String::new()));}
    });
    ui.separator();
    let mut delete=None;let mut duplicate=None;
    for (index,(key,value)) in rows.iter_mut().enumerate() {
        ui.push_id(("pair_row",tab,index),|ui|ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x=gap;
            let key_response=ui.add_sized([key_width,30.0],TextEdit::singleline(key).font(theme::font(Role::Mono)).desired_width(key_width-12.0).margin(egui::vec2(6.0,6.0)).hint_text("Key"));
            let masked=pair_value_masked(key,value,exclusions,session);
            // TextEdit chooses password mode before consuming events. Re-evaluate
            // paint after edits too, so a newly pasted token never flashes once.
            let mut layouter=|ui:&egui::Ui,text:&dyn egui::TextBuffer,_wrap_width:f32| {
                let raw=text.as_str();
                let display=if pair_value_masked(key,raw,exclusions,session) {"•".repeat(raw.chars().count())}else{raw.replace('\r',"␍").replace('\n',"↵")};
                ui.fonts_mut(|fonts|fonts.layout_no_wrap(display,theme::font(Role::Mono),ui.visuals().text_color()))
            };
            let value_response=ui.add_sized([value_width,30.0],TextEdit::singleline(value).password(masked).layouter(&mut layouter).font(theme::font(Role::Mono)).desired_width(value_width-12.0).margin(egui::vec2(6.0,6.0)).hint_text("Value"));
            if ui.add_sized([72.0,30.0],egui::Button::new(caption("Duplicate"))).clicked(){duplicate=Some(index);}
            let remove=ui.add_sized([46.0,30.0],egui::Button::new(caption("Delete")));
            if remove.clicked(){delete=Some(index);}
            #[cfg(test)] ui.ctx().data_mut(|d| {
                d.insert_temp(egui::Id::new(("pair_cell",tab,index,"key")),key_response.rect);
                d.insert_temp(egui::Id::new(("pair_cell",tab,index,"value")),value_response.rect);
                d.insert_temp(egui::Id::new(("pair_delete",tab,index)),remove.rect);
            });
            let _=(key_response,value_response);
        }));
        if key.contains(policy::REDACTED) || value.contains(policy::REDACTED) {ui.label(caption(&format!("Row {} · Re-entry required: replace or delete the omitted value.",index+1)).color(theme::ERROR));}
        else if key.trim().is_empty() && !value.is_empty() {ui.label(caption(&format!("Row {} · Key is required when Value is not empty.",index+1)).color(theme::ERROR));}
    }
    if let Some(index)=delete {rows.remove(index);}
    else if let Some(index)=duplicate {rows.insert(index+1,rows[index].clone());}
}
fn pair_snapshot(rows:&[(String,String)],label:&str)->Result<Vec<(String,String)>,String> {
    let mut pairs=Vec::new();
    for (index,(key,value)) in rows.iter().enumerate() {
        if key.trim().is_empty() {
            if value.is_empty() {continue;}
            return Err(format!("{label} row {}: Key is required when Value is not empty.",index+1));
        }
        pairs.push((key.clone(),value.clone()));
    }
    Ok(pairs)
}
fn exclusion_list(ui:&mut egui::Ui,label:&str,values:&mut Vec<String>,text:&mut String){
    ui.label(format!("{label} · comma-separated"));
    let response=ui.add(TextEdit::singleline(text).desired_width(f32::INFINITY));
    #[cfg(test)] ui.ctx().data_mut(|d|d.insert_temp(egui::Id::new(("exclusion_input",label)),response.rect));
    if response.changed(){*values=text.split(',').map(str::trim).filter(|v|!v.is_empty()).map(str::to_owned).collect();}
}
fn history_age(at:u64,now:u64)->String {
    let seconds=now.saturating_sub(at);
    if seconds<60 {"just now".into()}else if seconds<3600 {format!("{}m ago",seconds/60)}else if seconds<86400 {format!("{}h ago",seconds/3600)}else{format!("{}d ago",seconds/86400)}
}
fn tabs(ui:&mut egui::Ui, selected:&mut usize, labels:&[&str]) {
        for (index,label) in labels.iter().enumerate() {
            let active=*selected==index;
            let response=ui.add(egui::Button::new(RichText::new(*label).font(theme::font(Role::Caption)).color(if active {theme::TEXT} else {theme::MUTED})).frame(false).min_size(egui::vec2(48.0,28.0)));
            if response.clicked() {*selected=index;}
            if active {let y=response.rect.bottom();ui.painter().line_segment([egui::pos2(response.rect.left()+6.0,y),egui::pos2(response.rect.right()-6.0,y)],egui::Stroke::new(2.0,theme::TEXT));}
        }
}
impl Drop for ImdevApp {
    fn drop(&mut self) {
        if let Some(running)=self.running.take(){let _=running.cancel.send(true);running.task.abort();}
        if let Some(runtime)=self.runtime.take(){runtime.shutdown_background();}
    }
}
impl eframe::App for ImdevApp { fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) { self.show(ctx); } }

#[cfg(test)]
pub(crate) fn capture_auth_settings(app:&mut ImdevApp) {app.auth_origin="http://127.0.0.1:8080".into();app.auth_token="synthetic-preview-token".into();app.auth_settings=true;}
#[cfg(test)]
pub(crate) fn capture_restored(app:&mut ImdevApp,sensitive:bool) {
    let draft=RequestDraft{request:Request{method:"POST".into(),url:"http://127.0.0.1:8080/v1/nested/echo?view=full&view=summary".into(),query:vec![("tag".into(),"one".into()),("tag".into(),"two".into())],headers:vec![("Accept".into(),"application/json".into()),("X-Demo".into(),"synthetic-local".into())],body_type:"json".into(),body:if sensitive {"{\"profile\":{\"label\":\"synthetic private\"},\"mode\":\"local-only\"}"}else{"{\"profile\":{\"label\":\"Synthetic example\"},\"mode\":\"local-only\"}"}.into(),timeout_ms:45000},auth:AuthMode::Common,exclusions:Exclusions{query_keys:vec!["private_note".into()],header_names:vec!["X-Private".into()],json_pointers:if sensitive{vec!["/profile/label".into()]}else{vec![]},path_segments:vec![],body:false}};
    let snapshot=policy::snapshot(&draft,&Default::default()).unwrap();
    if let Some(store)=app.store.as_mut(){store.save("Synthetic nested request",&snapshot).unwrap();store.record(&snapshot,&Err("synthetic fixture".into())).unwrap();}
    app.install(snapshot);app.request_tab=if sensitive{2}else{0};
    app.session.origin="http://127.0.0.1:8080".into();app.session.token="synthetic-preview-token".into();
}
#[cfg(test)]
pub(crate) fn capture_pair_table(app:&mut ImdevApp,headers:bool) {
    capture_restored(app,false);
    app.query.push(("empty".into(),String::new()));
    if headers {
        app.headers.push(("Authorization".into(),"Bearer synthetic-preview-token".into()));
        app.headers.push(("X-Private".into(),policy::REDACTED.into()));
        app.request_tab=1;
    }
}
#[cfg(test)]
pub(crate) fn capture_app(ctx: &egui::Context) -> ImdevApp { ImdevApp::memory(ctx) }
#[cfg(test)]
pub(crate) fn capture_app_with_store(ctx:&egui::Context,path:std::path::PathBuf)->ImdevApp {ImdevApp::with_path(ctx,path)}
#[cfg(test)]
pub(crate) fn capture_response(app:&mut ImdevApp,request:Request,response:imdev::core::ResponseData) {
    app.query=request.query.clone(); app.headers=request.headers.clone(); app.request=request;
    app.displayed_body=response.body.clone(); app.response=Some(response); app.notice.clear();
}

pub fn headless_check() -> Result<(), String> {
    let ctx = egui::Context::default();
    let mut app = ImdevApp::memory(&ctx);
    let output = ctx.run(egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200.0,850.0))), ..Default::default() }, |ctx| app.show(ctx));
    fn text(shape: &egui::epaint::Shape, out: &mut String) {
        match shape {
            egui::epaint::Shape::Text(t) => { out.push_str(t.galley.text()); out.push('\n'); }
            egui::epaint::Shape::Vec(shapes) => for shape in shapes { text(shape, out); },
            _ => {}
        }
    }
    let mut labels = String::new();
    for clipped in &output.shapes { text(&clipped.shape, &mut labels); }
    for label in ["Imdev", "Saved templates", "History", "Request workspace", "Send", "Response", "Waiting for your first request", "General requests encrypted"] {
        if !labels.contains(label) { return Err("Missing startup widget".into()); }
    }
    if !ctx.style().visuals.dark_mode || ctx.zoom_factor() != 1.0 { return Err("Theme policy failed".into()); }
    for (style, role) in [(egui::TextStyle::Body,Role::Body), (egui::TextStyle::Button,Role::Button), (egui::TextStyle::Small,Role::Small), (egui::TextStyle::Monospace,Role::Mono), (egui::TextStyle::Heading,Role::Headline)] {
        if (ctx.style().text_styles[&style].size-role.size()).abs()>0.00001 { return Err("Typography policy failed".into()); }
    }
    Ok(())
}

pub fn pretty_json(body: &str) -> Result<String, String> {
    serde_json::from_str::<serde_json::Value>(body).and_then(|v| serde_json::to_string_pretty(&v)).map_err(|_| "Response is not valid JSON".into())
}
pub fn pairs_text(pairs: &[(String, String)]) -> String { pairs.iter().map(|(k,v)| format!("{k}: {v}")).collect::<Vec<_>>().join("\n") }
#[cfg(test)]
mod tests {
    use super::*;
    fn fixture_snapshot(request:Request)->Snapshot {policy::snapshot(&RequestDraft{request,..Default::default()},&Default::default()).unwrap()}
    fn label_rect(shapes: &[egui::epaint::ClippedShape], needle: &str) -> egui::Rect {
        fn find(shape: &egui::epaint::Shape, needle: &str) -> Option<egui::Rect> {
            match shape {
                egui::epaint::Shape::Text(t) if t.galley.text().contains(needle) => Some(egui::Rect::from_min_size(t.pos, t.galley.size())),
                egui::epaint::Shape::Vec(v) => v.iter().find_map(|s| find(s, needle)),
                _ => None,
            }
        }
        shapes.iter().find_map(|s| find(&s.shape, needle)).unwrap_or_else(|| panic!("missing {needle}"))
    }
    fn pair_frame(ctx:&egui::Context,app:&mut ImdevApp,width:f32,events:Vec<egui::Event>)->egui::FullOutput {
        ctx.run(egui::RawInput{screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(width,860.0))),events,..Default::default()},|ctx|app.show(ctx))
    }
    fn pair_click(ctx:&egui::Context,app:&mut ImdevApp,point:egui::Pos2) {
        for pressed in [true,false] {pair_frame(ctx,app,1280.0,vec![egui::Event::PointerMoved(point),egui::Event::PointerButton{pos:point,button:egui::PointerButton::Primary,pressed,modifiers:egui::Modifiers::NONE}]);}
    }
    #[test]
    fn compact_pair_table_add_action_is_visible_without_scrolling() {
        let ctx=egui::Context::default();let mut app=capture_app(&ctx);capture_pair_table(&mut app,false);
        let out=ctx.run(egui::RawInput{screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(760.0,720.0))),..Default::default()},|ctx|app.show(ctx));
        let add=label_rect(&out.shapes,"Add row");
        assert!(out.shapes.iter().any(|s|matches!(&s.shape,egui::epaint::Shape::Text(t) if t.galley.text()=="Add row") && s.clip_rect.contains_rect(add)),"Add row must be visible without scrolling");
    }
    #[test]
    fn old_snapshot_tuple_payload_reopens_into_table_without_migration() {
        let snapshot:Snapshot=serde_json::from_value(serde_json::json!({"draft":{"request":{"method":"POST","url":"http://127.0.0.1:1/nested?tag=raw","query":[["tag","one:two  "],["tag",""]],"headers":[["X-Demo"," a:b  "],["X-Demo",""]],"body_type":"text","body":"unchanged: body\n  ","timeout_ms":30000},"auth":"None","exclusions":{"query_keys":[],"header_names":[],"json_pointers":[],"path_segments":[],"body":false}},"missing":[]})).unwrap();
        let dir=tempfile::tempdir().unwrap();let path=dir.path().join("old.json");let ctx=egui::Context::default();
        {let mut app=ImdevApp::with_path(&ctx,path.clone());app.store.as_mut().unwrap().save("Old tuple vectors",&snapshot).unwrap();}
        let mut app=ImdevApp::with_path(&ctx,path);let restored=app.store.as_ref().unwrap().saved[0].snapshot.clone();assert!(app.restore(restored));
        assert_eq!(app.query,snapshot.draft.request.query);assert_eq!(app.headers,snapshot.draft.request.headers);assert_eq!(app.draft().unwrap(),snapshot.draft);
        app.request_tab=1;let out=pair_frame(&ctx,&mut app,1280.0,vec![]);pair_click(&ctx,&mut app,label_rect(&out.shapes,"Add row").center());assert_eq!(app.headers.len(),3);
        pair_frame(&ctx,&mut app,1280.0,vec![]);
        let point=ctx.data_mut(|d|d.get_temp::<egui::Rect>(egui::Id::new(("pair_cell",1,2,"value"))).unwrap().center());pair_click(&ctx,&mut app,point);
        pair_frame(&ctx,&mut app,1280.0,vec![egui::Event::Text("header: value  ".into())]);assert_eq!(app.headers[2].1,"header: value  ");
        assert!(!app.open_save());assert!(app.notice.contains("Headers row 3"));
        let point=ctx.data_mut(|d|d.get_temp::<egui::Rect>(egui::Id::new(("pair_delete",1,2))).unwrap().center());pair_click(&ctx,&mut app,point);assert_eq!(app.draft().unwrap(),snapshot.draft);
    }
    #[test]
    fn pasted_session_token_is_masked_in_the_insertion_frame() {
        let ctx=egui::Context::default();let mut app=capture_app(&ctx);app.query=vec![("ordinary".into(),String::new())];app.session.token="synthetic-pasted-copy".into();
        pair_frame(&ctx,&mut app,1280.0,vec![]);
        let point=ctx.data_mut(|d|d.get_temp::<egui::Rect>(egui::Id::new(("pair_cell",0,0,"value"))).unwrap().center());pair_click(&ctx,&mut app,point);
        let out=pair_frame(&ctx,&mut app,1280.0,vec![egui::Event::Text("prefix synthetic-pasted-copy".into())]);
        fn has_secret(shape:&egui::epaint::Shape)->bool {match shape {egui::epaint::Shape::Text(t)=>t.galley.text().contains("synthetic-pasted-copy"),egui::epaint::Shape::Vec(v)=>v.iter().any(has_secret),_=>false}}
        assert!(!out.shapes.iter().any(|s|has_secret(&s.shape)),"Insertion frame exposed copied session token");
        assert_eq!(app.query[0].1,"prefix synthetic-pasted-copy");
    }
    #[test]
    fn table_send_history_reopen_keeps_raw_query_and_additional_pairs_once() {
        use std::io::{Read,Write};
        let ctx=egui::Context::default();let dir=tempfile::tempdir().unwrap();let path=dir.path().join("history.json");
        let listener=std::net::TcpListener::bind("127.0.0.1:0").unwrap();listener.set_nonblocking(true).unwrap();
        let mut app=ImdevApp::with_path(&ctx,path.clone());app.request.url=format!("http://{}/echo?tag=raw&keep=a%20b",listener.local_addr().unwrap());
        app.query=vec![("tag".into(),"one:two  ".into()),("tag".into(),"".into())];
        app.headers=vec![("X-Demo".into(),"first".into()),("X-Demo".into(),"second".into())];
        let expected=app.request_snapshot().unwrap();
        for attempt in 0..2 {
            if attempt==1 {
                drop(app);app=ImdevApp::with_path(&ctx,path.clone());let snapshot=app.store.as_ref().unwrap().history[0].snapshot.clone();
                assert!(app.restore(snapshot));assert_eq!(app.request_snapshot().unwrap(),expected);assert!(listener.accept().is_err());assert!(app.running.is_none());
            }
            assert!(app.start(&ctx));let deadline=std::time::Instant::now()+std::time::Duration::from_secs(3);
            let mut stream=loop {match listener.accept(){Ok((s,_))=>break s,Err(_)=>{assert!(std::time::Instant::now()<deadline,"explicit Send did not reach loopback");std::thread::sleep(std::time::Duration::from_millis(5));}}};
            stream.set_nonblocking(false).unwrap();stream.set_read_timeout(Some(std::time::Duration::from_secs(3))).unwrap();let mut bytes=Vec::new();
            while !bytes.windows(4).any(|w|w==b"\r\n\r\n") {let mut buf=[0;4096];let n=stream.read(&mut buf).unwrap();assert!(n>0);bytes.extend_from_slice(&buf[..n]);}
            let wire=String::from_utf8(bytes).unwrap();assert_eq!(wire.lines().next().unwrap(),"GET /echo?tag=raw&keep=a%20b&tag=one%3Atwo++&tag= HTTP/1.1");
            let lower=wire.to_lowercase();let values=lower.lines().filter_map(|l|l.strip_prefix("x-demo: ")).collect::<Vec<_>>();assert_eq!(values,vec!["first","second"]);
            stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
            while app.busy {assert!(std::time::Instant::now()<deadline);app.poll_result();std::thread::sleep(std::time::Duration::from_millis(5));}
            assert_eq!(app.response.as_ref().unwrap().status,200);assert!(listener.accept().is_err());
        }
    }
    #[test]
    fn table_headers_crlf_and_duplicate_authorization_keep_existing_policy() {
        let ctx=egui::Context::default();let dir=tempfile::tempdir().unwrap();let mut app=ImdevApp::with_path(&ctx,dir.path().join("invalid.json"));app.request.url="http://127.0.0.1:1/".into();
        app.headers=vec![("X-Demo".into(),"synthetic\r\nInjected: value".into())];assert!(!app.open_save());assert!(!app.start(&ctx));assert!(app.running.is_none());
        app.headers=vec![("Authorization".into(),"one".into()),("authorization".into(),"two".into())];app.auth=AuthMode::None;
        assert!(!app.start(&ctx));assert!(app.notice.contains("Duplicate Authorization"));
    }
    #[test]
    fn sensitive_pair_values_mask_actual_paint_and_markers_show_reentry() {
        fn painted(shape:&egui::epaint::Shape,out:&mut String){match shape{egui::epaint::Shape::Text(t)=>{out.push_str(t.galley.text());out.push('\n');},egui::epaint::Shape::Vec(v)=>for s in v{painted(s,out)},_=>{}}}
        let ctx=egui::Context::default();let mut app=capture_app(&ctx);app.session.token="synthetic-copy".into();
        app.exclusions.query_keys=vec!["private_note".into()];app.exclusions.header_names=vec!["X-Private".into()];
        app.query=vec![("access_token".into(),"synthetic-query-secret".into()),("private_note".into(),"synthetic-private-query".into()),("ordinary".into(),"prefix synthetic-copy".into())];
        app.headers=vec![("Authorization".into(),"synthetic-manual-secret".into()),("X-Private".into(),"synthetic-private-header".into()),("Cookie".into(),policy::REDACTED.into())];
        for tab in 0..2 {app.request_tab=tab;let out=pair_frame(&ctx,&mut app,1280.0,vec![]);let mut text=String::new();for s in &out.shapes{painted(&s.shape,&mut text);}
            for secret in ["synthetic-query-secret","synthetic-private-query","synthetic-copy","synthetic-manual-secret","synthetic-private-header",policy::REDACTED] {assert!(!text.contains(secret),"sensitive value painted: {secret}");}
            if tab==1 {assert!(text.contains("Re-entry required"));}
        }
        app.request.url="http://127.0.0.1:1/".into();app.reviewed=true;assert!(!app.start(&ctx));assert!(app.notice.contains("Re-enter"));
    }
    #[test]
    fn pair_table_pointer_add_edit_duplicate_delete_and_compact_geometry() {
        let ctx=egui::Context::default();let mut app=capture_app(&ctx);
        let out=pair_frame(&ctx,&mut app,1280.0,vec![]);
        let add=label_rect(&out.shapes,"Add row");pair_click(&ctx,&mut app,add.center());assert_eq!(app.query.len(),1);
        for (cell,text) in [("key","tag"),("value","typed: colon  ")] {
            pair_frame(&ctx,&mut app,1280.0,vec![]);
            let point=ctx.data_mut(|d|d.get_temp::<egui::Rect>(egui::Id::new(("pair_cell",0,0,cell))).unwrap().center());
            pair_click(&ctx,&mut app,point);
            pair_frame(&ctx,&mut app,1280.0,vec![egui::Event::Text(text.into())]);
        }
        for _ in 0..3 {pair_frame(&ctx,&mut app,1280.0,vec![]);}
        assert_eq!(app.query,vec![("tag".to_string(),"typed: colon  ".to_string())]);
        let out=pair_frame(&ctx,&mut app,1280.0,vec![]);pair_click(&ctx,&mut app,label_rect(&out.shapes,"Duplicate").center());
        assert_eq!(app.query.len(),2);assert_eq!(app.query[0],app.query[1]);
        let point=ctx.data_mut(|d|d.get_temp::<egui::Rect>(egui::Id::new(("pair_delete",0,0))).unwrap().center());pair_click(&ctx,&mut app,point);assert_eq!(app.query.len(),1);
        for width in [1280.0,900.0,760.0] {
            let out=pair_frame(&ctx,&mut app,width,vec![]);for label in ["Key","Value","Add row","Delete"] {let rect=label_rect(&out.shapes,label);assert!(rect.right()<width-18.0,"{label}: {rect:?}");}
        }
    }
    #[test]
    fn pair_rows_ignore_blank_but_reject_missing_keys_without_mutating_editor() {
        let ctx=egui::Context::default();let dir=tempfile::tempdir().unwrap();let mut app=ImdevApp::with_path(&ctx,dir.path().join("rows.json"));
        app.request.url="http://127.0.0.1:1/".into();
        app.query=vec![("a".into(),"".into()),("".into(),"".into()),("a".into()," x:y  ".into())];
        assert_eq!(app.request_snapshot().unwrap().query,vec![("a".to_string(),"".to_string()),("a".into()," x:y  ".into())]);
        assert_eq!(app.query.len(),3);
        app.headers=vec![("  ".into(),"nonempty".into())];
        assert!(app.request_snapshot().unwrap_err().contains("Headers row 1"));
        assert!(!app.open_save());assert!(!app.start(&ctx));assert!(app.running.is_none());
        let invalid=Snapshot{draft:RequestDraft{request:Request{headers:app.headers.clone(),..Default::default()},..Default::default()},missing:vec![]};
        app.install(invalid);assert_eq!(app.headers[0].1,"nonempty");
    }
    #[test]
    fn restored_tuple_vectors_preserve_exact_values_and_order() {
        let ctx=egui::Context::default();let mut app=capture_app(&ctx);
        let request=Request{url:"http://127.0.0.1:1/path?tag=raw".into(),query:vec![("tag".into(),"one: two  ".into()),("tag".into(),"".into()),("other".into(),"line\nnext".into())],headers:vec![("X-Demo".into(),"a:b  ".into())],..Default::default()};
        app.install(Snapshot{draft:RequestDraft{request:request.clone(),..Default::default()},missing:vec![]});
        assert_eq!(app.request_snapshot().unwrap(),request);
        pair_frame(&ctx,&mut app,1280.0,vec![]);
        let rect=ctx.data_mut(|d|d.get_temp::<egui::Rect>(egui::Id::new(("pair_cell",0,2,"value"))).unwrap());
        assert!(rect.height()<=31.0,"Restored newline must not turn a single-line cell into multiline: {rect:?}");
        assert_eq!(app.request_snapshot().unwrap(),request);
    }
    #[test]
    fn actual_history_click_cancel_confirm_and_send_are_separate_boundaries() {
        use std::io::{Read,Write};
        let ctx=egui::Context::default();let dir=tempfile::tempdir().unwrap();let mut app=ImdevApp::with_path(&ctx,dir.path().join("state.json"));
        let listener=std::net::TcpListener::bind("127.0.0.1:0").unwrap();listener.set_nonblocking(true).unwrap();
        let origin=format!("http://{}",listener.local_addr().unwrap());let url=format!("{origin}/nested/echo?a=1&a=2");
        let draft=RequestDraft{request:Request{url:url.clone(),..Default::default()},auth:AuthMode::Common,..Default::default()};let snap=policy::snapshot(&draft,&Default::default()).unwrap();
        app.store.as_mut().unwrap().record(&snap,&Err("synthetic fixture".into())).unwrap();app.request.url="http://127.0.0.1:1/edited".into();
        fn frame(ctx:&egui::Context,app:&mut ImdevApp,events:Vec<egui::Event>)->egui::FullOutput {ctx.run(egui::RawInput{screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(1280.0,860.0))),events,..Default::default()},|ctx|app.show(ctx))}
        fn click(ctx:&egui::Context,app:&mut ImdevApp,label:&str){
            fn exact(shape:&egui::epaint::Shape,label:&str)->Option<egui::Rect>{match shape{egui::epaint::Shape::Text(t) if t.galley.text()==label=>Some(egui::Rect::from_min_size(t.pos,t.galley.size())),egui::epaint::Shape::Vec(v)=>v.iter().find_map(|s|exact(s,label)),_=>None}}
            let mut out=frame(ctx,app,vec![]);for _ in 0..2{out=frame(ctx,app,vec![]);}let pos=out.shapes.iter().find_map(|s|exact(&s.shape,label)).unwrap_or_else(||panic!("missing exact button {label}")).center();for pressed in [true,false]{frame(ctx,app,vec![egui::Event::PointerMoved(pos),egui::Event::PointerButton{pos,button:egui::PointerButton::Primary,pressed,modifiers:egui::Modifiers::NONE}]);}
        }
        let history_label=format!("GET {url}");click(&ctx,&mut app,&history_label);assert!(matches!(app.modal,Some(Modal::Replace(_))));assert!(listener.accept().is_err());
        click(&ctx,&mut app,"Cancel");assert!(app.modal.is_none(),"Cancel must dismiss confirmation");assert!(app.request.url.ends_with("edited"));assert!(listener.accept().is_err());
        click(&ctx,&mut app,&history_label);assert!(matches!(app.modal,Some(Modal::Replace(_))),"second history click must reopen dirty confirmation; url={}, notice={}",app.request.url,app.notice);click(&ctx,&mut app,"Confirm");assert_eq!(app.request.url,url);assert!(listener.accept().is_err());assert!(app.running.is_none());
        app.session=policy::AuthSession{origin,token:"synthetic-replaced-latest".into()};click(&ctx,&mut app,"Send");
        let deadline=std::time::Instant::now()+std::time::Duration::from_secs(3);let mut connection=None;
        while std::time::Instant::now()<deadline{if let Ok((s,_))=listener.accept(){connection=Some(s);break;}std::thread::sleep(std::time::Duration::from_millis(5));}
        let mut s=connection.expect("Only explicit Send reaches loopback");s.set_nonblocking(false).unwrap();s.set_read_timeout(Some(std::time::Duration::from_secs(3))).unwrap();let mut buf=[0;4096];let n=s.read(&mut buf).unwrap();assert!(String::from_utf8_lossy(&buf[..n]).contains("Bearer synthetic-replaced-latest"));s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
    }
    #[test]
    fn known_session_token_in_template_name_is_rejected() {
        let ctx=egui::Context::default();let dir=tempfile::tempdir().unwrap();let mut app=ImdevApp::with_path(&ctx,dir.path().join("state.json"));app.request.url="http://127.0.0.1:1/".into();app.session.token="synthetic-forbidden-name".into();
        assert!(app.open_save());app.save_name="copy synthetic-forbidden-name".into();assert!(!app.confirm_modal());assert!(app.notice.contains("Never put secrets"));assert!(app.store.as_ref().unwrap().saved.is_empty());
    }
    #[test]
    fn auth_none_allows_send_without_common_token_and_conflict_is_visible() {
        let ctx=egui::Context::default();let mut app=capture_app(&ctx);let listener=std::net::TcpListener::bind("127.0.0.1:0").unwrap();let origin=format!("http://{}",listener.local_addr().unwrap());app.request.url=format!("{origin}/");app.session.origin=origin;app.auth=AuthMode::Common;
        assert!(!app.start(&ctx));assert!(app.notice.contains("session token"));app.session.token="synthetic-token".into();app.headers=vec![("Authorization".into(),"synthetic-manual".into())];assert!(!app.start(&ctx));assert!(app.notice.contains("conflicts"));
        app.auth=AuthMode::None;app.session.token.clear();assert!(app.start(&ctx));
    }
    #[test]
    fn compact_restored_frame_explicitly_says_no_request_sent() {
        let ctx=egui::Context::default();let mut app=capture_app(&ctx);capture_restored(&mut app,false);
        let out=ctx.run(egui::RawInput{screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(900.0,720.0))),..Default::default()},|ctx|app.show(ctx));
        label_rect(&out.shapes,"Restored · not sent");
    }
    #[test]
    fn disabled_body_is_preserved_in_draft_and_invalid_path_exclusion_blocks_save() {
        let ctx=egui::Context::default();let mut app=capture_app(&ctx);app.request.body="synthetic remembered body".into();
        assert_eq!(app.request_snapshot().unwrap().body,"synthetic remembered body");
        app.exclusion_text[3]="1, invalid".into();assert!(app.draft().is_err());
    }
    #[test]
    fn exclusion_lists_keep_pending_separators_across_frames() {
        let ctx=egui::Context::default();let mut app=capture_app(&ctx);app.request_tab=3;
        let input=|events|egui::RawInput{screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(1280.0,860.0))),events,..Default::default()};
        let _=ctx.run(input(vec![]),|ctx|app.show(ctx));
        let point=ctx.data_mut(|d|d.get_temp::<egui::Rect>(egui::Id::new(("exclusion_input","Query keys"))).unwrap().center());
        for pressed in [true,false] {let _=ctx.run(input(vec![egui::Event::PointerMoved(point),egui::Event::PointerButton{pos:point,button:egui::PointerButton::Primary,pressed,modifiers:egui::Modifiers::NONE}]),|ctx|app.show(ctx));}
        for text in ["private",","," ","another"]{let _=ctx.run(input(vec![egui::Event::Text(text.into())]),|ctx|app.show(ctx));}
        assert_eq!(app.exclusions.query_keys,vec!["private","another"]);
    }
    #[test]
    fn legacy_records_are_partial_none_auth_and_original_file_unchanged() {
        let dir=tempfile::tempdir().unwrap();let path=dir.path().join("legacy.json");
        let mut legacy=imdev::store::Store::open(path.clone()).unwrap();legacy.save("Legacy",&Request{url:"http://127.0.0.1:1/discarded?a=1".into(),..Default::default()}).unwrap();
        let before=std::fs::read(&path).unwrap();let ctx=egui::Context::default();let mut app=ImdevApp::with_path(&ctx,path.clone());
        let request=app.store.as_ref().unwrap().legacy_saved[0].request.clone();assert!(app.load_legacy(request));
        assert_eq!(app.auth,AuthMode::None);assert!(!app.missing.is_empty());assert!(app.notice.contains("irrecoverable"));
        app.modal=Some(Modal::ClearHistory);assert!(app.confirm_modal());assert_eq!(std::fs::read(path).unwrap(),before);
        let out=ctx.run(egui::RawInput::default(),|ctx|app.show(ctx));label_rect(&out.shapes,"Legacy · read-only");label_rect(&out.shapes,"Clears new history only");
    }
    #[test]
    fn restored_frame_shows_auth_exclusions_and_review_warning() {
        let ctx=egui::Context::default();let mut app=capture_app(&ctx);
        capture_restored(&mut app,true);app.request_tab=3;
        let out=ctx.run(egui::RawInput{screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(1280.0,860.0))),..Default::default()},|ctx|app.show(ctx));
        for label in ["Common", "Exclusions", "Query keys", "Header names", "JSON pointers", "Path segments", "Exclude full body", "I reviewed"]{label_rect(&out.shapes,label);}
    }
    #[test]
    fn excluded_restore_requires_review_and_markers_still_block_send() {
        let ctx=egui::Context::default();let mut app=capture_app(&ctx);
        let mut draft=RequestDraft{request:Request{url:"http://127.0.0.1:1/nested/private".into(),..Default::default()},..Default::default()};draft.exclusions.path_segments=vec![2];
        let snap=policy::snapshot(&draft,&Default::default()).unwrap();assert!(app.restore(snap));
        assert!(!app.start(&ctx));assert!(app.notice.contains("Review"));app.reviewed=true;
        assert!(!app.start(&ctx));assert!(app.notice.contains("Re-enter"));assert!(app.running.is_none());
    }
    #[test]
    fn settings_persist_origin_only_and_failure_is_explicit_session_only() {
        let ctx=egui::Context::default();let dir=tempfile::tempdir().unwrap();let path=dir.path().join("state.json");let mut app=ImdevApp::with_path(&ctx,path.clone());
        app.auth_origin="HTTP://127.0.0.1:8080/".into();app.auth_token="synthetic-session-token".into();assert!(app.apply_auth());
        assert_eq!(app.session.origin,"http://127.0.0.1:8080");let next=ImdevApp::with_path(&ctx,path);assert_eq!(next.session.origin,app.session.origin);assert!(next.session.token.is_empty());
        app.store=None;app.auth_origin="http://127.0.0.1:9090".into();app.auth_token="synthetic-next".into();assert!(app.apply_auth());assert!(app.notice.contains("session only"));
        app.auth_origin="http://127.0.0.1:9090/path".into();assert!(!app.apply_auth());assert_eq!(app.session.origin,"http://127.0.0.1:9090");
    }
    #[test]
    fn encrypted_save_retains_general_request_and_restart_origin_not_token() {
        let ctx=egui::Context::default();let dir=tempfile::tempdir().unwrap();let path=dir.path().join("state.json");
        let mut app=ImdevApp::with_path(&ctx,path.clone());
        app.request.url="http://127.0.0.1:8080/v1/nested/echo?a=1&a=2".into();app.query=vec![("tag".into(),"one".into()),("tag".into(),"two".into())];
        app.session.origin="http://127.0.0.1:8080".into();app.session.token="synthetic-token".into();
        assert!(app.open_save());app.save_name="Synthetic example".into();assert!(app.confirm_modal());
        let next=ImdevApp::with_path(&ctx,path.clone());
        assert_eq!(next.store.as_ref().unwrap().saved[0].snapshot.draft.request.url,app.request.url);
        assert!(!path.exists());assert!(imdev::repository::Repository::encrypted_path_for(&path).exists());
        assert!(next.session.token.is_empty());
    }
    #[test]
    fn send_resolves_latest_common_token_but_running_keeps_original_snapshot() {
        use std::io::{Read,Write};
        let ctx=egui::Context::default();let mut app=capture_app(&ctx);
        let listener=std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let origin=format!("http://{}",listener.local_addr().unwrap());
        app.request.url=format!("{origin}/v1/echo");app.auth=AuthMode::Common;
        app.session=policy::AuthSession{origin,token:"synthetic-latest-token".into()};
        let server=std::thread::spawn(move||{let(mut s,_)=listener.accept().unwrap();s.set_read_timeout(Some(std::time::Duration::from_secs(3))).unwrap();let mut buf=[0;4096];let n=s.read(&mut buf).unwrap();let wire=String::from_utf8_lossy(&buf[..n]).to_string();s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();wire});
        assert!(app.start(&ctx));
        let saved=serde_json::to_string(&app.running.as_ref().unwrap().snapshot).unwrap();
        assert!(!saved.contains("synthetic-latest-token"));assert!(!saved.contains("Authorization"));assert!(saved.contains("Common"));
        assert!(server.join().unwrap().to_lowercase().contains("authorization: bearer synthetic-latest-token"));
    }
    #[test]
    fn restore_requires_dirty_confirmation_and_never_sends() {
        use imdev::policy::{self,AuthMode,Exclusions,RequestDraft,AuthSession};
        let ctx=egui::Context::default();let mut app=capture_app(&ctx);
        let listener=std::net::TcpListener::bind("127.0.0.1:0").unwrap();listener.set_nonblocking(true).unwrap();
        let draft=RequestDraft{request:Request{method:"POST".into(),url:format!("http://{}/v1/nested/echo?a=1&a=2",listener.local_addr().unwrap()),body_type:"json".into(),body:"{\"nested\":{\"value\":3}}".into(),query:vec![("tag".into(),"one".into()),("tag".into(),"two".into())],headers:vec![("X-Demo".into(),"synthetic".into())],timeout_ms:45000,..Default::default()},auth:AuthMode::Common,exclusions:Exclusions{query_keys:vec!["sensitive".into()],..Default::default()}};
        let snapshot=policy::snapshot(&draft,&AuthSession::default()).unwrap();
        app.request.url="http://127.0.0.1:1/edited".into();
        assert!(!app.restore(snapshot.clone()));assert!(matches!(app.modal,Some(Modal::Replace(_))));
        app.modal=None;assert!(app.request.url.ends_with("edited"));assert!(listener.accept().is_err());
        assert!(!app.restore(snapshot));assert!(app.confirm_modal());
        assert_eq!(app.draft().unwrap(),draft);assert!(app.running.is_none());assert!(listener.accept().is_err());
    }
    #[test]
    fn common_settings_mask_token_in_actual_paint() {
        let ctx=egui::Context::default(); let mut app=capture_app(&ctx);
        app.auth_token="synthetic-preview-token".into(); app.auth_origin="http://127.0.0.1:8080".into(); app.auth_settings=true;
        for _ in 0..2 {let _=ctx.run(egui::RawInput::default(),|ctx|app.show(ctx));}
        let out=ctx.run(egui::RawInput{screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(1280.0,860.0))),..Default::default()},|ctx|app.show(ctx));
        label_rect(&out.shapes,"Common authentication");
        fn texts(s:&egui::epaint::Shape,out:&mut String){match s{egui::epaint::Shape::Text(t)=>out.push_str(t.galley.text()),egui::epaint::Shape::Vec(v)=>for s in v{texts(s,out)},_=>{}}}
        let mut painted=String::new();for s in &out.shapes{texts(&s.shape,&mut painted)}
        assert!(!painted.contains("synthetic-preview-token")); assert!(painted.contains("session memory only"));
    }
    #[test]
    fn label_control_rows_share_fonts_and_compact_visible_heights() {
        fn text_info(shapes:&[egui::epaint::ClippedShape],needle:&str)->(egui::Rect,egui::FontId){
            fn find(shape:&egui::epaint::Shape,needle:&str)->Option<(egui::Rect,egui::FontId)>{match shape{
                egui::epaint::Shape::Text(t) if t.galley.text()==needle || (needle=="30000" && t.galley.text().chars().filter(|c|c.is_ascii_digit()).collect::<String>()==needle)=>Some((egui::Rect::from_min_size(t.pos,t.galley.size()),t.galley.job.sections[0].format.font_id.clone())),
                egui::epaint::Shape::Vec(items)=>items.iter().find_map(|s|find(s,needle)),_=>None}}
            shapes.iter().find_map(|s|find(&s.shape,needle)).unwrap_or_else(||panic!("missing {needle}"))
        }
        for(width,height)in[(1280.0,860.0),(900.0,720.0),(760.0,580.0)]{
            let ctx=egui::Context::default();let mut app=capture_app(&ctx);
            let out=ctx.run(egui::RawInput{screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(width,height))),..Default::default()},|ctx|app.show(ctx));
            for(a,b)in[("History","Clear history"),("Timeout (ms)","30000")]{
                let(ra,fa)=text_info(&out.shapes,a);let(rb,fb)=text_info(&out.shapes,b);
                assert_eq!(fa,fb,"same row must use the same measured font: {a}/{b}");
                assert!((ra.height()-rb.height()).abs()<0.5,"line heights differ: {ra:?} {rb:?}");
                assert!((ra.center().y-rb.center().y).abs()<0.5,"text centers differ: {ra:?} {rb:?}");
            }
            for id in ["Clear history","Timeout input"]{
                let(visible,hit)=ctx.data_mut(|d|d.get_temp::<(egui::Rect,egui::Rect)>(egui::Id::new(("compact_row_geometry",id))).expect("measured control geometry"));
                assert!(visible.height()<=22.5,"visible control must not retain oversized padding: {visible:?}");
                assert!(hit.height()>=28.0,"keep a comfortable pointer target: {hit:?}");
            }
        }
    }
    #[test]
    fn compact_timeout_keeps_keyboard_editing_and_range() {
        let ctx=egui::Context::default();let mut value=30000_u64;
        let mut run=|events:Vec<egui::Event>| {let _=ctx.run(egui::RawInput{events,..Default::default()},|ctx| {egui::CentralPanel::default().show(ctx,|ui|{theme::control_row(ui,|ui|{ui.label(RichText::new("Timeout (ms)").font(theme::font(Role::Caption)));theme::timeout_control(ui,&mut value,true);});});});};
        run(vec![]);
        let point=ctx.data_mut(|d|d.get_temp::<(egui::Rect,egui::Rect)>(egui::Id::new(("compact_row_geometry","Timeout input"))).unwrap().1.center());
        run(vec![egui::Event::PointerMoved(point),egui::Event::PointerButton{pos:point,button:egui::PointerButton::Primary,pressed:true,modifiers:egui::Modifiers::NONE}]);
        run(vec![egui::Event::PointerButton{pos:point,button:egui::PointerButton::Primary,pressed:false,modifiers:egui::Modifiers::NONE}]);
        run(vec![egui::Event::Text("45000".into())]);
        run(vec![egui::Event::Key{key:egui::Key::Enter,physical_key:None,pressed:true,repeat:false,modifiers:egui::Modifiers::NONE}]);
        drop(run);
        assert_eq!(value,45000);
        let _=ctx.run(egui::RawInput::default(),|ctx| {egui::CentralPanel::default().show(ctx,|ui|{theme::timeout_control(ui,&mut value,true);});});
        let(visible,hit)=ctx.data_mut(|d|d.get_temp::<(egui::Rect,egui::Rect)>(egui::Id::new(("compact_row_geometry","Timeout input"))).unwrap());
        assert_eq!(visible.height(),22.0);assert_eq!(hit.height(),30.0);
        value=999999;
        let _=ctx.run(egui::RawInput::default(),|ctx| {egui::CentralPanel::default().show(ctx,|ui|{theme::timeout_control(ui,&mut value,true);});});
        assert_eq!(value,120000);
        value=1;
        let _=ctx.run(egui::RawInput::default(),|ctx| {egui::CentralPanel::default().show(ctx,|ui|{theme::timeout_control(ui,&mut value,true);});});
        assert_eq!(value,100);
    }
    #[test]
    fn compact_clear_action_opens_confirmation_without_clearing_history() {
        let dir=tempfile::tempdir().unwrap();let ctx=egui::Context::default();let mut app=ImdevApp::with_path(&ctx,dir.path().join("state.json"));
        app.store.as_mut().unwrap().record(&fixture_snapshot(Request{url:"http://127.0.0.1:1/".into(),..Default::default()}),&Err("synthetic fixture".into())).unwrap();
        let input=|events|egui::RawInput{screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(1280.0,860.0))),events,..Default::default()};
        let _=ctx.run(input(vec![]),|ctx|app.show(ctx));
        let point=ctx.data_mut(|d|d.get_temp::<(egui::Rect,egui::Rect)>(egui::Id::new(("compact_row_geometry","Clear history"))).unwrap().1.center());
        let _=ctx.run(input(vec![egui::Event::PointerMoved(point),egui::Event::PointerButton{pos:point,button:egui::PointerButton::Primary,pressed:true,modifiers:egui::Modifiers::NONE}]),|ctx|app.show(ctx));
        let _=ctx.run(input(vec![egui::Event::PointerButton{pos:point,button:egui::PointerButton::Primary,pressed:false,modifiers:egui::Modifiers::NONE}]),|ctx|app.show(ctx));
        assert!(matches!(app.modal,Some(Modal::ClearHistory)));assert_eq!(app.store.as_ref().unwrap().history.len(),1);
        let _=ctx.run(input(vec![egui::Event::Key{key:egui::Key::Escape,physical_key:None,pressed:true,repeat:false,modifiers:egui::Modifiers::NONE}]),|ctx|app.show(ctx));
        assert!(app.modal.is_none());assert_eq!(app.store.as_ref().unwrap().history.len(),1);
    }
    #[test]
    fn proposal_wide_inspectors_are_side_by_side_and_compact_stacks() {
        for (width, height) in [(1280.0,860.0),(900.0,720.0),(760.0,580.0)] {
            let ctx=egui::Context::default(); let mut app=ImdevApp::memory(&ctx);
            let out=ctx.run(egui::RawInput { screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(width,height))), ..Default::default() }, |ctx|app.show(ctx));
            let request=label_rect(&out.shapes,"Request editor");
            let response=label_rect(&out.shapes,"Response");
            let (a,b)=ctx.data_mut(|d|(d.get_temp::<egui::Rect>(egui::Id::new("request_actual")).unwrap(),d.get_temp::<egui::Rect>(egui::Id::new("response_actual")).unwrap()));
            assert!(!a.intersects(b),"cards overlap: {a:?} {b:?}");
            for r in [a,b] {assert!(r.right()<=width-18.0 && (height<680.0 || r.bottom()<=height-30.0),"card outside viewport: {r:?}");}
            if height<680.0 {let viewport=ctx.data_mut(|d|d.get_temp::<egui::Rect>(egui::Id::new("inspectors_viewport")).unwrap());assert!(viewport.bottom()<=height-30.0);}
            if width>1000.0 { assert!((request.top()-response.top()).abs()<4.0,"wide inspector headings must align: {request:?} {response:?}"); assert!(request.right()<response.left()); }
            else { assert!(request.bottom()<response.top()); }
            for label in ["Send","Query","Pretty JSON","Copy tab"] {
                let r=label_rect(&out.shapes,label); assert!(r.left()>=220.0 && r.right()<=width && r.bottom()<=height,"{label}: {r:?}");
            }
        }
    }
    #[test]
    fn populated_editors_and_loading_states_stay_inside_panels() {
        for (width,height) in [(1280.0,860.0),(900.0,720.0),(760.0,580.0)] {
            let ctx=egui::Context::default(); let mut app=capture_app(&ctx);
            let request=Request {url:"http://127.0.0.1:8080/v1/echo".into(),body_type:"json".into(),body:"{\"fixture\":true}".into(),..Default::default()};
            capture_response(&mut app,request,imdev::core::ResponseData {status:200,status_text:"OK".into(),headers:vec![("Content-Type".into(),"application/json".into())],body:"{\"fixture\":true}".into(),elapsed_ms:3,truncated:false});
            for tab in 0..3 {
                app.request_tab=tab;app.response_tab=tab%2;app.busy=tab==2;
                let out=ctx.run(egui::RawInput {screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(width,height))),..Default::default()},|ctx|app.show(ctx));
                let (a,b)=ctx.data_mut(|d|(d.get_temp::<egui::Rect>(egui::Id::new("request_actual")).unwrap(),d.get_temp::<egui::Rect>(egui::Id::new("response_actual")).unwrap()));
                assert!(!a.intersects(b));
                for r in [a,b] {assert!(r.right()<=width-18.0 && (height<680.0 || r.bottom()<=height-30.0),"tab {tab}: {r:?}");}
                for label in ["Pretty JSON","Copy tab",if app.busy {"Cancel"} else {"Send"}] {let r=label_rect(&out.shapes,label);assert!(r.right()<=width-18.0 && r.bottom()<=height-30.0);}
            }
            assert!(app.store.is_none());
        }
    }
    #[test]
    fn minimum_window_scrolls_inspectors_without_hiding_send() {
        let ctx=egui::Context::default();let mut app=capture_app(&ctx);
        for frame in 0..6 {
            let events=if frame==1 {vec![egui::Event::PointerMoved(egui::pos2(720.0,520.0)),egui::Event::MouseWheel{unit:egui::MouseWheelUnit::Point,delta:egui::vec2(0.0,-800.0),modifiers:egui::Modifiers::NONE}]}else{vec![]};
            let out=ctx.run(egui::RawInput{screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(760.0,580.0))),events,time:Some(frame as f64*0.2),..Default::default()},|ctx|app.show(ctx));
            let send=label_rect(&out.shapes,"Send");assert!(send.bottom()<200.0);
        }
        let(offset,viewport,response)=ctx.data_mut(|d|(d.get_temp::<f32>(egui::Id::new("inspectors_scroll_offset")).unwrap(),d.get_temp::<egui::Rect>(egui::Id::new("inspectors_viewport")).unwrap(),d.get_temp::<egui::Rect>(egui::Id::new("response_actual")).unwrap()));
        assert!(offset>100.0,"Native wheel input must scroll the inspector viewport: {offset}");
        assert!(response.bottom()<=viewport.bottom()+2.0,"Response must be reachable by scrolling: {response:?} {viewport:?}");
    }
    #[test]
    fn history_keeps_timestamp_context_as_readable_relative_time() {
        assert_eq!(history_age(1000,1000),"just now");
        assert_eq!(history_age(1000,1000+2*60),"2m ago");
        assert_eq!(history_age(1000,1000+2*60*60),"2h ago");
        assert_eq!(history_age(1000,1000+2*24*60*60),"2d ago");
        assert_eq!(history_age(2000,1000),"just now");
    }
    #[test]
    fn command_enter_in_a_real_frame_starts_native_request() {
        let ctx=egui::Context::default(); let mut app=ImdevApp::memory(&ctx);
        let listener=std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        app.request.url=format!("http://{}/",listener.local_addr().unwrap());
        let input=egui::RawInput { events: vec![egui::Event::Key { key:egui::Key::Enter, physical_key:None, pressed:true, repeat:false, modifiers:egui::Modifiers::COMMAND }], ..Default::default() };
        let _=ctx.run(input,|ctx|app.show(ctx)); assert!(app.busy);
    }
    #[test]
    fn destructive_library_actions_require_explicit_confirmation() {
        let dir=tempfile::tempdir().unwrap(); let mut app=ImdevApp::with_path(&egui::Context::default(),dir.path().join("state.json"));
        let request=Request { url:"https://example.com/".into(), ..Default::default() };
        app.store.as_mut().unwrap().save("Keep",&fixture_snapshot(request.clone())).unwrap();
        let id=app.store.as_ref().unwrap().saved[0].id.clone();
        app.modal=Some(Modal::Delete(id.clone())); app.modal=None;
        assert_eq!(app.store.as_ref().unwrap().saved.len(),1);
        app.modal=Some(Modal::Delete(id)); assert!(app.confirm_modal());
        assert!(app.store.as_ref().unwrap().saved.is_empty());
        app.store.as_mut().unwrap().record(&fixture_snapshot(request),&Err("cancelled".into())).unwrap();
        app.modal=Some(Modal::ClearHistory); assert_eq!(app.store.as_ref().unwrap().history.len(),1);
        assert!(app.confirm_modal()); assert!(app.store.as_ref().unwrap().history.is_empty());
    }
    #[test]
    fn save_requires_confirmation_and_only_persists_safe_template() {
        let dir=tempfile::tempdir().unwrap(); let path=dir.path().join("state.json");
        let mut app=ImdevApp::with_path(&egui::Context::default(),path.clone());
        app.request.url="https://example.com/private?token=secret".into();
        app.headers=vec![("Authorization".into(),"***".into())];
        assert!(app.open_save());
        assert!(app.store.as_ref().unwrap().saved.is_empty());
        app.modal=None; assert!(!app.confirm_modal());
        assert!(app.open_save()); app.save_name="Example".into(); assert!(app.confirm_modal());
        let entry=&app.store.as_ref().unwrap().saved[0];
        assert_eq!(entry.snapshot.draft.request.url,"https://example.com/private?token=__IMDEV_REENTER_SECRET__"); assert_eq!(entry.snapshot.draft.request.headers[0].1, policy::REDACTED);
        let disk=std::fs::read(imdev::repository::Repository::encrypted_path_for(&path)).unwrap();assert!(!disk.windows(6).any(|w|w==b"secret"));
    }
    #[test]
    fn completed_http_result_updates_response_and_persists_safe_history() {
        use std::io::{Read, Write};
        let ctx = egui::Context::default(); let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json"); let mut app = ImdevApp::with_path(&ctx, path.clone());
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        app.request.url = format!("http://{}/secret?token=secret", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || { let (mut s,_) = listener.accept().unwrap(); s.set_read_timeout(Some(std::time::Duration::from_secs(3))).unwrap(); let mut buf=[0;4096]; let _=s.read(&mut buf); s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"ok\":true}").unwrap(); });
        assert!(app.start(&ctx));
        let deadline=std::time::Instant::now()+std::time::Duration::from_secs(3);
        while app.busy && std::time::Instant::now()<deadline { app.poll_result(); std::thread::sleep(std::time::Duration::from_millis(5)); }
        server.join().unwrap();
        assert_eq!(app.response.as_ref().unwrap().status,200);
        assert_eq!(app.displayed_body,"{\"ok\":true}");
        assert_eq!(app.store.as_ref().unwrap().history.len(),1);
        let disk=std::fs::read(imdev::repository::Repository::encrypted_path_for(&path)).unwrap(); assert!(!disk.windows(6).any(|w|w==b"secret"));
    }
    #[test]
    fn corrupt_store_is_preserved_and_saving_is_disabled_with_warning() {
        let dir = tempfile::tempdir().unwrap(); let path = dir.path().join("state.json");
        std::fs::write(&path, "corrupt").unwrap();
        let app = ImdevApp::with_path(&egui::Context::default(), path.clone());
        assert!(app.store.is_none());
        assert!(app.storage_warning.contains("disabled"));
        assert_eq!(std::fs::read_to_string(path).unwrap(), "corrupt");
    }
    #[test]
    fn asynchronous_send_is_single_flight_and_cancel_returns_to_idle() {
        let ctx = egui::Context::default();
        let mut app = ImdevApp::memory(&ctx);
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        app.request.url = format!("http://{}/", listener.local_addr().unwrap());
        assert!(app.start(&ctx));
        assert!(app.busy);
        assert!(!app.start(&ctx));
        app.running.as_ref().unwrap().cancel.send(true).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while app.busy && std::time::Instant::now() < deadline { app.poll_result(); std::thread::sleep(std::time::Duration::from_millis(5)); }
        assert!(!app.busy);
        assert!(app.response.is_none());
    }
    #[test]
    fn json_formatting_is_explicit_and_rejects_non_json() {
        assert_eq!(pretty_json("{\"ok\":true}").unwrap(), "{\n  \"ok\": true\n}");
        assert!(pretty_json("plain text").is_err());
    }
    #[test]
    fn loading_or_new_request_is_blocked_while_running() {
        let mut app = ImdevApp::memory(&egui::Context::default());
        app.request.url = "https://example.com/active".into();
        app.busy = true;
        assert!(!app.load_request(Request::default()));
        assert_eq!(app.request.url, "https://example.com/active");
    }
    #[test]
    fn editor_snapshot_parses_duplicate_pairs_and_rejects_bad_lines() {
        let mut app = ImdevApp::memory(&egui::Context::default());
        app.request.url = "https://example.com/".into();
        app.query = vec![("a".into(),"1".into()),("a".into(),"2".into())];
        assert_eq!(app.request_snapshot().unwrap().query.len(), 2);
        app.headers = vec![("".into(),"not a pair".into())];
        assert!(app.request_snapshot().is_err());
    }
    #[test]
    fn real_headless_frame_has_startup_labels_dark_theme_and_adjusted_fonts() { assert_eq!(headless_check(), Ok(())); }
    #[test]
    fn pair_editor_preserves_duplicates_and_empty_values() {
        assert_eq!(pairs_text(&[("a".into(), "1".into()), ("a".into(), "".into())]), "a: 1\na: ");
    }
}
