//! A ponte entre as telas e o serviço: um comando só (`call`) com uma tabela fixa de métodos do
//! `ServiceProxy`, e os sinais do serviço virando eventos. As telas nunca falam com o Google.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use futures_util::stream::{BoxStream, StreamExt, select_all};
use gdl_ipc::{ServiceError, ServiceProxy, nobody_there, service_lost};
use serde::Serialize;
use serde_json::{Value, json};
use zbus::Connection;

/// De quanto em quanto tempo procurar o serviço quando ele some.
pub const RECONNECT: Duration = Duration::from_secs(2);

/// O erro que a tela recebe: `name` é o nome curto do erro do serviço (`NotLoggedIn`), ou
/// `Unavailable` (serviço fora do ar), `UnknownMethod` e `InvalidArgs` (da própria ponte).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UiError {
    pub name: String,
    pub message: String,
}

impl UiError {
    fn new(name: &str, message: impl Into<String>) -> UiError {
        UiError {
            name: name.into(),
            message: message.into(),
        }
    }
}

/// Quem recebe os eventos (nome e conteúdo): na janela, o `emit` do Tauri.
pub type Emit = Arc<dyn Fn(&'static str, Value) + Send + Sync>;

pub struct Bridge {
    service: ServiceProxy<'static>,
    up: AtomicBool,
}

/// Os argumentos da chamada, um objeto JSON com os nomes dos parâmetros.
struct Args<'a>(&'a Value);

impl<'a> Args<'a> {
    fn get(&self, key: &str) -> Result<&'a Value, UiError> {
        self.0
            .get(key)
            .ok_or_else(|| UiError::new("InvalidArgs", format!("falta o argumento {key}")))
    }

    fn str(&self, key: &str) -> Result<&'a str, UiError> {
        self.get(key)?.as_str().ok_or_else(|| wrong(key))
    }

    fn u64(&self, key: &str) -> Result<u64, UiError> {
        self.get(key)?.as_u64().ok_or_else(|| wrong(key))
    }

    fn bool(&self, key: &str) -> Result<bool, UiError> {
        self.get(key)?.as_bool().ok_or_else(|| wrong(key))
    }

    /// `[[chave, valor], …]`.
    fn pairs(&self, key: &str) -> Result<Vec<(&'a str, &'a str)>, UiError> {
        fn pair(v: &Value) -> Option<(&str, &str)> {
            match v.as_array()?.as_slice() {
                [k, v] => k.as_str().zip(v.as_str()),
                _ => None,
            }
        }
        self.get(key)?
            .as_array()
            .and_then(|list| list.iter().map(pair).collect())
            .ok_or_else(|| wrong(key))
    }
}

fn wrong(key: &str) -> UiError {
    UiError::new("InvalidArgs", format!("argumento {key} com tipo errado"))
}

impl Bridge {
    pub async fn new(conn: &Connection) -> zbus::Result<Bridge> {
        // Sem a releitura, a janela mostra "o serviço não está respondendo" (o aviso já cobre a falha).
        let _ = gdl_ipc::ensure_activatable(conn).await;
        Ok(Bridge {
            service: ServiceProxy::new(conn).await?,
            up: AtomicBool::new(true),
        })
    }

    /// O barramento de sessão (o mesmo do serviço), para os portais.
    pub fn connection(&self) -> &Connection {
        self.service.inner().connection()
    }

    /// O serviço respondeu na última tentativa.
    pub fn is_up(&self) -> bool {
        self.up.load(Ordering::Relaxed)
    }

    /// Chama `method` (o nome do método no `ServiceProxy`) com `args` e devolve a resposta em
    /// JSON.
    pub async fn call(&self, method: &str, args: Value) -> Result<Value, UiError> {
        let a = Args(&args);
        let s = &self.service;
        // Espera a resposta do serviço e a passa para JSON.
        macro_rules! j {
            ($call:expr) => {
                to_json($call.await)
            };
        }
        let reply = match method {
            "begin_login" => j!(s.begin_login(a.str("client_id")?, a.str("client_secret")?)),
            "status" => j!(s.status()),
            "account_info" => j!(s.account_info()),
            "activity" => j!(s.activity()),
            "errors" => j!(s.errors()),
            "notifications" => j!(s.notifications()),
            "mark_notifications_read" => j!(s.mark_notifications_read()),
            "dismiss_notification" => j!(s.dismiss_notification(a.u64("id")?)),
            "invoke_notification_action" => {
                j!(s.invoke_notification_action(a.u64("id")?, a.str("key")?))
            }
            "notification_settings" => j!(s.notification_settings()),
            "set_notification_enabled" => {
                j!(s.set_notification_enabled(a.str("kind")?, a.bool("enabled")?))
            }
            "retry" => j!(s.retry(a.str("item_id")?)),
            "cancel_upload" => j!(s.cancel_upload(a.str("item_id")?)),
            "pin" => j!(s.pin(a.str("path")?)),
            "unpin" => j!(s.unpin(a.str("path")?)),
            "pause" => j!(s.pause()),
            "resume" => j!(s.resume()),
            "list_lost_and_found" => j!(s.list_lost_and_found()),
            "offline_summary" => j!(s.offline_summary()),
            "offline_items" => j!(s.offline_items(a.str("folder_id")?)),
            "unpin_item" => j!(s.unpin_item(a.str("item_id")?)),
            "clear_cache" => j!(s.clear_cache()),
            "clear_offline_files" => j!(s.clear_offline_files()),
            "search" => j!(s.search(a.str("query")?)),
            "recent_items" => j!(s.recent_items()),
            "open_item" => j!(s.open_item(a.str("item_id")?, a.str("target")?)),
            "web_link" => j!(s.web_link(a.str("item_id")?)),
            "shared_with_me" => j!(s.shared_with_me()),
            "add_shortcut" => j!(s.add_shortcut(a.str("item_id")?)),
            "logout" => j!(s.logout(a.str("mode")?, a.str("dir")?)),
            "preferences" => j!(s.preferences()),
            "set_preferences" => j!(s.set_preferences(&a.pairs("values")?)),
            "reset_preferences" => j!(s.reset_preferences()),
            "restart" => j!(s.restart()),
            "quit" => j!(s.quit()),
            _ => return Err(UiError::new("UnknownMethod", method)),
        };
        self.settle(reply)
    }

    /// A pasta do Drive (a montagem), direto do `Status` do serviço.
    pub async fn mount_point(&self) -> Result<String, UiError> {
        let status = self.settle(self.service.status().await)?;
        Ok(status.mount_point)
    }

    /// Passa o erro para a página e anota se o serviço respondeu: qualquer resposta (mesmo um
    /// erro do serviço) mostra que ele está no ar.
    fn settle<T>(&self, reply: Result<T, ServiceError>) -> Result<T, UiError> {
        let reply = reply.map_err(ui_error);
        let down = matches!(&reply, Err(e) if e.name == UNAVAILABLE);
        self.up.store(!down, Ordering::Relaxed);
        reply
    }

    /// Repassa os sinais do serviço para `emit` enquanto a conexão durar.
    pub async fn forward_signals(&self, emit: Emit) -> zbus::Result<()> {
        let s = &self.service;
        let (status, activity, notifications, preferences, login) = futures_util::try_join!(
            s.receive_status_changed(),
            s.receive_activity_changed(),
            s.receive_notifications_changed(),
            s.receive_preferences_changed(),
            s.receive_login_finished(),
        )?;
        let login = login.map(|signal| {
            let payload = signal.args().map_or(
                Value::Null,
                |a| json!({ "ok": a.ok, "email": a.email, "error": a.error }),
            );
            ("login-finished", payload)
        });
        let streams: Vec<BoxStream<'static, (&'static str, Value)>> = vec![
            status.map(named("status-changed")).boxed(),
            activity.map(named("activity-changed")).boxed(),
            notifications.map(named("notifications-changed")).boxed(),
            preferences.map(named("preferences-changed")).boxed(),
            login.boxed(),
        ];
        let mut all = select_all(streams);
        tokio::spawn(async move {
            while let Some((name, payload)) = all.next().await {
                emit(name, payload);
            }
        });
        Ok(())
    }

    /// Enquanto o serviço estiver fora, tenta a cada `every`; quando ele volta, emite
    /// `service-up` (as telas recarregam o que mostram). Quando o nome do serviço perde o dono,
    /// emite `service-down` na hora (as telas mostram o aviso sem esperar uma chamada falhar).
    pub async fn watch(self: Arc<Self>, every: Duration, emit: Emit) {
        let mut ticker = tokio::time::interval(every);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        // Sem os avisos do barramento, a queda só aparece quando uma chamada falhar.
        let mut lost = service_lost(self.service.inner().connection())
            .await
            .unwrap_or_else(|_| futures_util::stream::pending().boxed());
        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    if self.is_up() {
                        continue;
                    }
                    let _ = self.call("status", Value::Null).await;
                    if self.is_up() {
                        emit("service-up", Value::Null);
                    }
                }
                Some(_) = lost.next() => {
                    self.up.store(false, Ordering::Relaxed);
                    emit("service-down", Value::Null);
                }
            }
        }
    }
}

/// Um sinal sem argumentos vira o evento `name`.
fn named<T>(name: &'static str) -> impl FnMut(T) -> (&'static str, Value) {
    move |_| (name, Value::Null)
}

/// O nome do erro de serviço fora do ar.
const UNAVAILABLE: &str = "Unavailable";

fn ui_error(e: ServiceError) -> UiError {
    let name = e.short_name();
    match e {
        ServiceError::ZBus(e) if unavailable(&e) => UiError::new(UNAVAILABLE, e.to_string()),
        e => UiError::new(name, e.into_message()),
    }
}

/// O serviço não está lá (nunca subiu, saiu, está reiniciando ou travou).
fn unavailable(e: &zbus::Error) -> bool {
    nobody_there(e)
        || matches!(e, zbus::Error::InputOutput(_))
        || matches!(e, zbus::Error::MethodError(name, ..)
            if name.as_str() == "org.freedesktop.DBus.Error.NoReply")
}

fn to_json<T: Serialize>(reply: Result<T, ServiceError>) -> Result<Value, ServiceError> {
    reply.map(|v| serde_json::to_value(v).unwrap_or(Value::Null))
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use gdl_ipc::testing::{PrivateBus, p2p_pair};
    use gdl_ipc::{BUS_NAME, OBJECT_PATH, Status};
    use zbus::object_server::SignalEmitter;

    use super::*;

    /// Um serviço de mentira, com só o que os testes usam.
    struct FakeService;

    #[zbus::interface(name = "io.github.junior9283.GDriveDesktopLinux1.Service")]
    impl FakeService {
        async fn status(&self) -> Result<Status, ServiceError> {
            Err(ServiceError::NotLoggedIn("nenhuma conta conectada".into()))
        }

        async fn pin(&self, path: &str) -> Result<(), ServiceError> {
            if path == "/Meu Drive/a" {
                Ok(())
            } else {
                Err(ServiceError::NotFound(path.into()))
            }
        }

        #[zbus(signal)]
        async fn status_changed(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;
    }

    async fn bridge_with_fake() -> (Connection, Bridge) {
        let (server, client) = p2p_pair().await;
        server
            .object_server()
            .at(OBJECT_PATH, FakeService)
            .await
            .unwrap();
        (server, Bridge::new(&client).await.unwrap())
    }

    #[tokio::test]
    async fn bridge_rejects_unknown_method() {
        let (_server, bridge) = bridge_with_fake().await;

        let e = bridge.call("drop_everything", json!({})).await.unwrap_err();
        assert_eq!(e.name, "UnknownMethod");

        let e = bridge.call("pin", json!({})).await.unwrap_err();
        assert_eq!(e.name, "InvalidArgs");
        let e = bridge.call("pin", json!({ "path": 3 })).await.unwrap_err();
        assert_eq!(e.name, "InvalidArgs");
        assert_eq!(
            bridge.call("pin", json!({ "path": "/Meu Drive/a" })).await,
            Ok(Value::Null)
        );
    }

    #[tokio::test]
    async fn bridge_maps_dbus_error_name() {
        let (_server, bridge) = bridge_with_fake().await;

        let e = bridge.call("status", Value::Null).await.unwrap_err();

        assert_eq!(e, UiError::new("NotLoggedIn", "nenhuma conta conectada"));
        let e = bridge
            .call("pin", json!({ "path": "/x" }))
            .await
            .unwrap_err();
        assert_eq!((e.name.as_str(), e.message.as_str()), ("NotFound", "/x"));
        assert!(bridge.is_up());
    }

    #[tokio::test]
    async fn signals_become_events() {
        let (server, bridge) = bridge_with_fake().await;
        let seen = Arc::new(Mutex::new(Vec::new()));
        let emit: Emit = Arc::new({
            let seen = seen.clone();
            move |name, _| seen.lock().unwrap().push(name)
        });
        bridge.forward_signals(emit).await.unwrap();

        let iface = server
            .object_server()
            .interface::<_, FakeService>(OBJECT_PATH)
            .await
            .unwrap();
        FakeService::status_changed(iface.signal_emitter())
            .await
            .unwrap();

        for _ in 0..200 {
            if !seen.lock().unwrap().is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(*seen.lock().unwrap(), ["status-changed"]);
    }

    #[tokio::test]
    async fn service_down_gives_unavailable() {
        let bus = PrivateBus::start();
        let bridge = Arc::new(Bridge::new(&bus.connect().await).await.unwrap());

        let e = bridge.call("status", Value::Null).await.unwrap_err();
        assert_eq!(e.name, UNAVAILABLE);
        assert!(!bridge.is_up());

        // O serviço sobe: a ponte percebe sozinha e avisa as telas.
        let (tx, mut up) = tokio::sync::mpsc::unbounded_channel();
        let emit: Emit = Arc::new(move |name, _| {
            let _ = tx.send(name);
        });
        tokio::spawn(bridge.clone().watch(Duration::from_millis(50), emit));
        let service = bus.connect().await;
        service
            .object_server()
            .at(OBJECT_PATH, FakeService)
            .await
            .unwrap();
        service.request_name(BUS_NAME).await.unwrap();

        let event = tokio::time::timeout(Duration::from_secs(5), up.recv()).await;
        assert_eq!(event.ok().flatten(), Some("service-up"));
        assert!(bridge.is_up());

        // O serviço sai: o aviso vem do barramento, sem nenhuma chamada falhar antes.
        service.release_name(BUS_NAME).await.unwrap();
        let event = tokio::time::timeout(Duration::from_secs(5), up.recv()).await;
        assert_eq!(event.ok().flatten(), Some("service-down"));
        assert!(!bridge.is_up());
    }
}
