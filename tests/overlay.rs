//! Source-level guard for the transparent overlay contract; live compositor
//! checks are still needed to verify mapping, keyboard focus, and presentation.

#[test]
fn picker_is_a_transparent_overlay_with_a_bounded_card() {
    let qml = include_str!("../Snipboard.qml");
    let (_, surface) = qml.split_once("    PanelWindow {").expect("layer-shell picker");
    let (window, card) = surface.split_once("        BorderSurface {").unwrap();
    assert!(window.contains("color: \"transparent\""));
    assert!(window.contains("WlrLayershell.namespace: \"omarchy-snipboard\""));
    assert!(window.contains("WlrKeyboardFocus.Exclusive"));
    assert!(window.contains("onClicked: root.close()"));
    assert!(!window.contains("Rectangle {"), "no screen-covering background");
    assert!(!qml.contains("Color.menu.scrim"));
    assert!(!qml.contains("FloatingWindow {"));
    assert!(card.contains("anchors.centerIn: parent"));
    assert!(card.contains("width: Math.min(Style.space(900), panel.width - Style.gapsOut * 2)"));
    assert!(card.contains("height: Math.min(Style.space(600), panel.height - Style.gapsOut * 2)"));
}

#[test]
fn picker_columns_do_not_follow_image_size_hints() {
    let qml = include_str!("../Snipboard.qml");
    let (_, history) = qml.split_once("id: historyColumn").unwrap();
    let (history, _) = history.split_once("Caption {").unwrap();
    for constraint in ["minimumWidth", "preferredWidth", "maximumWidth"] {
        assert!(history.contains(&format!("Layout.{constraint}: card.width * 0.55")), "left column must stay fixed and larger");
    }
    let (_, preview) = qml.split_once("// Preview: exact text").unwrap();
    let (preview, _) = preview.split_once("ScrollView {").unwrap();
    assert!(preview.contains("Layout.minimumWidth: 0"));
    assert!(preview.contains("Layout.preferredWidth: 0"));
    let (_, viewport) = qml.split_once("id: imageViewport").unwrap();
    let (viewport, _) = viewport.split_once("Caption {").unwrap();
    assert!(viewport.contains("clip: true"));
    assert!(viewport.contains("Image {\n                                anchors.fill: parent"), "image must fit inside a layout-neutral viewport");
    assert!(viewport.contains("fillMode: Image.PreserveAspectFit"));
}
