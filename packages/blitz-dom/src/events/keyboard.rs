use crate::{BaseDocument, node::GeneratedTextInputEvent, util::ACTION_MOD};
use blitz_traits::node_id::NodeId;
use blitz_traits::{
    SmolStr,
    events::{BlitzInputEvent, BlitzKeyEvent, DomEventData},
};
use keyboard_types::{Key, Modifiers, NamedKey};
use markup5ever::local_name;

pub(super) enum KeyboardOrTextInputEvent {
    KeyPress(BlitzKeyEvent),
    AppleStandardKeyBinding(SmolStr),
}

pub(crate) fn handle_key_or_input_event(
    doc: &mut BaseDocument,
    target: NodeId,
    event: KeyboardOrTextInputEvent,
) {
    if let KeyboardOrTextInputEvent::KeyPress(event) = &event {
        // Enter on a focused button or link activates it, as a click does.
        if event.state.is_pressed()
            && !event.is_composing
            && event.key == Key::Named(NamedKey::Enter)
            && activates_by_key(doc, target, Activation::Enter)
        {
            queue_activation_click(doc, target, event.modifiers);
            return;
        }
        if event.key == Key::Named(NamedKey::Tab) {
            let backwards = event.modifiers.contains(Modifiers::SHIFT);
            if backwards {
                doc.focus_prev_node();
            } else {
                doc.focus_next_node();
            }
            return;
        }

        // Handle copy (Ctrl+C/Cmd+C) for text selection when no text input is focused
        if event.state.is_pressed() {
            let action_mod = event.modifiers.contains(ACTION_MOD);
            if action_mod {
                if let Key::Character(c) = &event.key {
                    if c.to_lowercase() == "c" {
                        // Check if we have a text selection (and no focused text input)
                        let has_focused_text_input = doc.focus_node_id.is_some_and(|id| {
                            doc.get_node(id)
                                .and_then(|n| n.element_data())
                                .is_some_and(|e| e.text_input_data().is_some())
                        });

                        if !has_focused_text_input {
                            if let Some(text) = doc.get_selected_text() {
                                let _ = doc.shell_provider.set_clipboard_text(text);
                                return;
                            }
                        }
                    }
                }
            }
        }
    }

    if let Some(node_id) = doc.focus_node_id {
        if target != node_id {
            return;
        }

        let node = &mut doc.nodes[node_id];
        let Some(element_data) = node.element_data_mut() else {
            return;
        };

        if let Some(input_data) = element_data.text_input_data_mut() {
            let generated_event = match event {
                KeyboardOrTextInputEvent::KeyPress(blitz_key_event) => input_data
                    .apply_keypress_event(
                        &mut doc.font_ctx.lock().unwrap(),
                        &mut doc.layout_ctx,
                        &*doc.shell_provider,
                        blitz_key_event,
                    ),
                KeyboardOrTextInputEvent::AppleStandardKeyBinding(command) => input_data
                    .apply_apple_standard_keybinding(
                        &mut doc.font_ctx.lock().unwrap(),
                        &mut doc.layout_ctx,
                        &*doc.shell_provider,
                        &command,
                    ),
            };

            if let Some(generated_event) = generated_event {
                doc.apply_generated_text_input_event(node_id, generated_event);
            }
        }
    }
}

impl BaseDocument {
    pub(crate) fn apply_generated_text_input_event(
        &mut self,
        node_id: NodeId,
        event: GeneratedTextInputEvent,
    ) {
        let node = &mut self.nodes[node_id];
        let element_data = node
            .element_data_mut()
            .expect("apply_generated_text_input_event called on a node that is not an element");
        let input_data = element_data
            .text_input_data_mut()
            .expect("apply_generated_text_input_event called on a node that is not a text input");

        match event {
            GeneratedTextInputEvent::Input => {
                let value = input_data.editor.raw_text().to_string();
                self.queue_ua_event(node_id, DomEventData::Input(BlitzInputEvent { value }));
                self.shell_provider.request_redraw();
            }
            GeneratedTextInputEvent::Select | GeneratedTextInputEvent::PreEditChange => {
                self.shell_provider.request_redraw();
            }
            GeneratedTextInputEvent::Submit => {
                // TODO: Generate submit event that can be handled by script
                implicit_form_submission(self, node_id);
            }
        }
    }
}

/// https://html.spec.whatwg.org/multipage/form-control-infrastructure.html#field-that-blocks-implicit-submission
fn implicit_form_submission(doc: &BaseDocument, text_target: NodeId) {
    let Some(form_owner_id) = doc.controls_to_form.get(&text_target) else {
        return;
    };
    if doc
        .controls_to_form
        .iter()
        .filter(|(_control_id, form_id)| *form_id == form_owner_id)
        .filter_map(|(control_id, _)| doc.nodes[*control_id].element_data())
        .filter(|element_data| {
            element_data.attr(local_name!("type")).is_some_and(|t| {
                matches!(
                    t,
                    "text"
                        | "search"
                        | "email"
                        | "url"
                        | "tel"
                        | "password"
                        | "date"
                        | "month"
                        | "week"
                        | "time"
                        | "datetime-local"
                        | "number"
                )
            })
        })
        .count()
        > 1
    {
        return;
    }

    doc.submit_form(*form_owner_id, *form_owner_id);
}

/// Which key activates an element: Enter on its keydown, or Space on its keyup.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Activation {
    Enter,
    Space,
}

/// Whether the focused `target` is activated by `key`, as a browser's buttons, links and summaries
/// are: Enter for a button, a link or a summary; Space for a button, a checkbox, a radio or a summary.
/// <https://html.spec.whatwg.org/multipage/interaction.html#activation>
pub(crate) fn activates_by_key(doc: &BaseDocument, target: NodeId, key: Activation) -> bool {
    if doc.focus_node_id != Some(target) {
        return false;
    }
    let Some(element) = doc.get_node(target).and_then(|n| n.element_data()) else {
        return false;
    };
    if element.has_attr(local_name!("disabled")) {
        return false;
    }
    let input_type = element.attr(local_name!("type")).map(|t| t.to_ascii_lowercase());
    match element.name.local {
        local_name!("button") | local_name!("summary") => true,
        local_name!("a") => key == Activation::Enter && element.has_attr(local_name!("href")),
        local_name!("input") => match input_type.as_deref() {
            Some("button" | "submit" | "reset" | "image") => true,
            Some("checkbox" | "radio") => key == Activation::Space,
            _ => false,
        },
        _ => false,
    }
}

/// Queue the click a key's activation fires: a synthetic one, at no pointer position.
pub(crate) fn queue_activation_click(doc: &mut BaseDocument, target: NodeId, mods: Modifiers) {
    use blitz_traits::events::{
        BlitzPointerEvent, BlitzPointerId, MouseEventButton, PointerCoords, PointerDetails,
    };
    let coords = PointerCoords { page_x: 0.0, page_y: 0.0, screen_x: 0.0, screen_y: 0.0, client_x: 0.0, client_y: 0.0 };
    doc.queue_ua_event(
        target,
        DomEventData::Click(BlitzPointerEvent {
            id: BlitzPointerId::Mouse,
            is_primary: true,
            coords,
            button: MouseEventButton::Main,
            buttons: Default::default(),
            mods,
            details: PointerDetails::default(),
            element: Default::default(),
            active_pointers: Default::default(),
        }),
    );
}
