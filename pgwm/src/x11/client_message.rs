use crate::error::Result;
use pgwm_core::push_heapless;

use x11rb::properties::WmHintsCookie;
use x11rb::{
    properties::WmHints,
    protocol::xproto::{ClientMessageEvent, PropertyNotifyEvent, Window},
};

use crate::x11::call_wrapper::{SupportedAtom, WmState};

use super::{
    call_wrapper::CallWrapper,
    cookies::{ClassConvertCookie, FallbackNameConvertCookie},
};

pub(crate) fn convert_property_change(
    call_wrapper: &mut CallWrapper,
    event: PropertyNotifyEvent,
) -> Result<Option<PropertyChangeMessage>> {
    if let Some(resolved) = call_wrapper.resolve_atom(event.atom) {
        match resolved.intern_atom {
            SupportedAtom::WmClass => Ok(Some(PropertyChangeMessage::ClassName((
                event.window,
                call_wrapper.get_class_names(event.window)?,
            )))),
            SupportedAtom::WmName | SupportedAtom::NetWmName => Ok(Some(
                PropertyChangeMessage::Name((event.window, call_wrapper.get_name(event.window)?)),
            )),
            SupportedAtom::WmHints => {
                let hints = WmHints::get(call_wrapper.inner_mut(), event.window)?;
                Ok(Some(PropertyChangeMessage::Hints((event.window, hints))))
            }
            SupportedAtom::WmState => {
                let state = call_wrapper.get_state(event.window)?;
                Ok(Some(PropertyChangeMessage::WmStateChange((
                    event.window,
                    state,
                ))))
            }
            _ => Ok(None),
        }
    } else {
        Ok(None)
    }
}

pub(crate) fn convert_message(
    call_wrapper: &mut CallWrapper,
    event: ClientMessageEvent,
) -> Result<Option<ClientMessage>> {
    let request_atom = event.type_;
    if let Some(resolved) = call_wrapper.resolve_atom(request_atom) {
        match resolved.intern_atom {
            SupportedAtom::NetWmState => {
                pgwm_core::debug!("Got request to update wm state");
                interpret_state(call_wrapper, event)
            }
            SupportedAtom::NetActiveWindow | SupportedAtom::NetWmStateDemandsAttention => {
                let current_active = event.data.as_data32()[2];
                if current_active != 0 {
                    pgwm_core::debug!(
                        "Got request to switch internal focus from {current_active} to {}",
                        event.window
                    );
                }
                Ok(Some(ClientMessage::RequestActiveWindow(event.window)))
            }
            SupportedAtom::NetCloseWindow => Ok(Some(ClientMessage::CloseWindow(event.window))),
            SupportedAtom::NetRequestFrameExtents => {
                Ok(Some(ClientMessage::RequestSetExtents(event.window)))
            }
            _ => Ok(None),
        }
    } else {
        Ok(None)
    }
}

fn interpret_state(
    call_wrapper: &mut CallWrapper,
    event: ClientMessageEvent,
) -> Result<Option<ClientMessage>> {
    let parts = event.data.as_data32();
    let mut state_changes: heapless::Vec<ChangeAction, 3> = heapless::Vec::new();
    let action = parts[0];
    let change_type = ChangeType::from_number(action);
    // Last one is the source indication
    for atom in parts.into_iter().take(parts.len() - 1).skip(1) {
        if let Some(resolved) = call_wrapper.resolve_atom(atom) {
            pgwm_core::debug!("Resolved atom to {resolved:?}");
            match resolved.intern_atom {
                SupportedAtom::NetWmStateFullscreen => {
                    push_heapless!(
                        state_changes,
                        ChangeAction {
                            change_type,
                            state_change: StateChange::Fullscreen,
                        }
                    )?;
                }
                SupportedAtom::NetWmStateModal => {
                    push_heapless!(
                        state_changes,
                        ChangeAction {
                            change_type,
                            state_change: StateChange::Modal,
                        }
                    )?;
                }
                SupportedAtom::NetWmStateDemandsAttention => {
                    push_heapless!(
                        state_changes,
                        ChangeAction {
                            change_type,
                            state_change: StateChange::DemandAttention,
                        }
                    )?;
                }
                _ => (),
            }
        }
    }
    if state_changes.is_empty() {
        Ok(None)
    } else {
        Ok(Some(ClientMessage::StateChange((
            event.window,
            state_changes,
        ))))
    }
}

#[derive(Debug)]
pub(crate) enum ClientMessage {
    RequestActiveWindow(Window),
    RequestSetExtents(Window),
    CloseWindow(Window),
    StateChange((Window, heapless::Vec<ChangeAction, 3>)),
}

pub(crate) enum PropertyChangeMessage {
    Hints((Window, WmHintsCookie)),
    ClassName((Window, ClassConvertCookie)),
    Name((Window, FallbackNameConvertCookie)),
    WmStateChange((Window, Option<WmState>)),
}

#[derive(Debug, Copy, Clone)]
pub(crate) struct ChangeAction {
    pub(crate) change_type: ChangeType,
    pub(crate) state_change: StateChange,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum ChangeType {
    Add,
    Remove,
    Toggle,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum StateChange {
    Modal,
    Fullscreen,
    DemandAttention,
}

impl ChangeType {
    fn from_number(num: u32) -> Self {
        match num {
            1 => ChangeType::Add,
            2 => ChangeType::Toggle,
            // Client error if this isn't 1, might aswell remove
            _ => ChangeType::Remove,
        }
    }
}
