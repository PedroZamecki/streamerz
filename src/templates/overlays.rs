//! Overlays that need no actions: a group with settings and an HTML file. The data comes from Streamerz's
//! built-in overlay topics (`ad`, `viewers`). They are starting points to copy and edit.
use super::Made;
use crate::groups::{text_file, Def, Kind, Meta};
use serde_json::{json, Value};

fn def(name: &str, label: &str, kind: Kind, default: Value, description: &str) -> Def {
    Def {
        name: name.into(),
        label: label.into(),
        kind,
        default,
        description: description.into(),
        ..Default::default()
    }
}

/// Shows while Twitch plays an ad.
pub fn ad() -> Made {
    let meta = Meta {
        description: "Shows a badge and the time left while Twitch plays an ad. Copy the URL of ad.html into an OBS browser source."
            .into(),
        variables: vec![
            def("label", "Badge text", Kind::Text, json!("Ad"), "The word on the badge"),
            def("accent", "Badge color", Kind::Text, json!("#fbbf24"), "Any CSS color"),
            def("size", "Text size", Kind::Integer, json!(34), "In pixels"),
        ],
        files: vec![text_file("ad.html", include_str!("../../assets/overlays/ad.html"))],
        ..Default::default()
    };
    Made {
        groups: vec![("Ad overlay".into(), meta)],
        ..Default::default()
    }
}

/// The live viewers of every connected platform, and their sum, counting up and down.
pub fn viewers() -> Made {
    let meta = Meta {
        description: "The live viewers of each connected platform and the total, animated. Copy the URL of viewers.html into an OBS browser source."
            .into(),
        variables: vec![
            def("showTotal", "Show the total", Kind::Boolean, json!(true), ""),
            def("showPlatforms", "Show each platform", Kind::Boolean, json!(true), ""),
            def("totalLabel", "Total label", Kind::Text, json!("Viewers"), ""),
            def("animationMs", "Animation time", Kind::Integer, json!(800), "In milliseconds"),
            def("accent", "Total color", Kind::Text, json!("#a78bfa"), "Any CSS color"),
        ],
        files: vec![text_file("viewers.html", include_str!("../../assets/overlays/viewers.html"))],
        ..Default::default()
    };
    Made {
        groups: vec![("Viewer overlay".into(), meta)],
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installing_adds_the_group_with_its_page_and_settings() {
        for (made, group, file) in [
            (ad(), "Ad overlay", "ad.html"),
            (viewers(), "Viewer overlay", "viewers.html"),
        ] {
            let mut c = crate::Config::default();
            made.add_to(&mut c).unwrap();
            assert!(crate::groups::meta(&c, group).files.iter().any(|f| f.name == file));
            assert!(!crate::groups::values(&c, group).is_empty());
            assert!(crate::overlays::list(&c).iter().any(|(g, f)| g == group && f == file));
            let (status, kind, body) =
                crate::overlays::file(&c, &format!("/overlays/{}/{file}", group.replace(' ', "%20")));
            assert_eq!(status, 200);
            assert!(kind.starts_with("text/html"));
            let page = String::from_utf8(body).unwrap();
            assert!(page.contains("window.streamerz") && page.contains("streamerz.on("));
        }
    }

    #[test]
    fn a_group_with_only_an_overlay_exports_and_imports() {
        let mut c = crate::Config::default();
        ad().add_to(&mut c).unwrap();
        let mut v = crate::groups::values(&c, "Ad overlay");
        v.insert("label".into(), serde_json::json!("Pausa"));
        crate::groups::set_values(&mut c, "Ad overlay", v).unwrap();
        // The group holds no action: ticking it in the export still carries its page and settings.
        assert!(c.actions.actions.is_empty());
        let x = crate::edit::export_with(&c, &[], &["Ad overlay".into()], ["Ad", "", "", ""]);
        let x: crate::model::Obj = crate::model::read_export(&crate::model::write_export(&x)).unwrap();
        assert!(crate::edit::summary(&x).contains("1 groups, 1 files, 0 actions"));
        // Nothing ticked as a group: an action export carries no group.
        let none = crate::edit::export_with(&c, &[], &[], ["", "", "", ""]);
        assert!(none["streamerz"]["groups"].as_object().unwrap().is_empty());
        let mut d = crate::Config::default();
        crate::edit::import(&mut d, &x).unwrap();
        assert_eq!(
            crate::groups::meta(&d, "Ad overlay"),
            crate::groups::meta(&c, "Ad overlay")
        );
        assert_eq!(crate::groups::values(&d, "Ad overlay")["label"], "Pausa");
        assert!(crate::overlays::list(&d)
            .iter()
            .any(|(g, f)| g == "Ad overlay" && f == "ad.html"));
        let (status, _, body) = crate::overlays::file(&d, "/overlays/Ad%20overlay/ad.html");
        assert_eq!(status, 200);
        assert!(String::from_utf8(body).unwrap().contains("\"Pausa\""));
    }
}
