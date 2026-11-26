use crate::model::Obj;
use serde_json::Value;

pub(crate) fn user(args: &mut Obj, platform: &str, id: Value, login: &str, display: &str) {
    args.insert("user".into(), display.into());
    args.insert("userName".into(), login.into());
    args.insert("userId".into(), id);
    args.insert("userType".into(), platform.into());
}

pub(crate) fn broadcaster(args: &mut Obj, login_key: &str, id: Value, login: impl Into<String>, display: Option<&str>) {
    if let Some(display) = display {
        args.insert("broadcastUser".into(), display.into());
    }
    args.insert(login_key.into(), Value::String(login.into()));
    args.insert("broadcastUserId".into(), id);
}
