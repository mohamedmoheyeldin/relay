//! Windows UI Automation: find, read, invoke and fill controls.
#![allow(unsafe_code)]

use windows::core::BSTR;
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationExpandCollapsePattern,
    IUIAutomationInvokePattern, IUIAutomationSelectionItemPattern, IUIAutomationTogglePattern,
    IUIAutomationTreeWalker, IUIAutomationValuePattern, UIA_ExpandCollapsePatternId,
    UIA_InvokePatternId, UIA_SelectionItemPatternId, UIA_TogglePatternId, UIA_ValuePatternId,
};
use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

use crate::types::{InputEvent, MouseButton, OsError, Rect, Result};
use crate::ui::{matches, outline_line, rank, role_name, UiElement, UiSelector};
use crate::win::{scan_windows, WindowsDesktop};

/// Stop walking a tree after this many nodes so a huge app cannot stall the caller.
const NODE_BUDGET: usize = 6_000;
const MAX_DEPTH: usize = 28;

fn fail<E: std::fmt::Display>(what: &str, e: E) -> OsError {
    OsError::Platform(format!("{what}: {e}"))
}

struct Session {
    automation: IUIAutomation,
    walker: IUIAutomationTreeWalker,
}

impl Session {
    fn new() -> Result<Self> {
        // SAFETY: COM initialisation per thread is idempotent; S_FALSE just means already initialised.
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let automation: IUIAutomation =
                CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
                    .map_err(|e| fail("UI Automation", e))?;
            let walker = automation
                .ControlViewWalker()
                .map_err(|e| fail("tree walker", e))?;
            Ok(Self { automation, walker })
        }
    }

    /// The window to search: a title match, or the foreground window.
    fn root(&self, window: Option<&str>) -> Result<IUIAutomationElement> {
        let hwnd = match window.map(str::trim).filter(|w| !w.is_empty()) {
            Some(title) => {
                let needle = title.to_lowercase();
                scan_windows()?
                    .into_iter()
                    .find(|(_, w)| w.title.to_lowercase().contains(&needle))
                    .map(|(h, _)| h)
                    .ok_or_else(|| {
                        OsError::InvalidArgument(format!("no window title contains {title:?}"))
                    })?
            }
            // SAFETY: GetForegroundWindow has no preconditions.
            None => unsafe { GetForegroundWindow() },
        };
        if hwnd == HWND::default() {
            return Err(OsError::Platform("there is no foreground window".into()));
        }
        // SAFETY: `hwnd` is a live window handle from the OS.
        unsafe { self.automation.ElementFromHandle(hwnd) }.map_err(|e| fail("window element", e))
    }

    fn read(&self, el: &IUIAutomationElement) -> UiElement {
        // SAFETY: property reads on a valid element; failures fall back to empty values.
        unsafe {
            let name = el.CurrentName().map(|b| b.to_string()).unwrap_or_default();
            let ct = el.CurrentControlType().map(|c| c.0).unwrap_or(0);
            let automation_id = el
                .CurrentAutomationId()
                .map(|b| b.to_string())
                .unwrap_or_default();
            let r = el.CurrentBoundingRectangle().unwrap_or_default();
            let enabled = el.CurrentIsEnabled().map(|b| b.as_bool()).unwrap_or(false);
            let value = el
                .GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
                .ok()
                .and_then(|p| p.CurrentValue().ok())
                .map(|b| b.to_string())
                .filter(|v| !v.is_empty());
            UiElement {
                name,
                role: role_name(ct).to_string(),
                automation_id,
                rect: Rect {
                    x: r.left,
                    y: r.top,
                    w: (r.right - r.left).max(0) as u32,
                    h: (r.bottom - r.top).max(0) as u32,
                },
                enabled,
                value,
            }
        }
    }

    /// Depth-first walk of `root`'s descendants, calling `visit(element, depth)`; return false to stop.
    fn walk(
        &self,
        root: &IUIAutomationElement,
        mut visit: impl FnMut(&IUIAutomationElement, usize) -> bool,
    ) {
        let mut stack: Vec<(IUIAutomationElement, usize)> = Vec::new();
        // SAFETY: tree navigation on valid elements.
        unsafe {
            if let Ok(first) = self.walker.GetFirstChildElement(root) {
                stack.push((first, 0));
            }
            let mut seen = 0usize;
            while let Some((el, depth)) = stack.pop() {
                seen += 1;
                if seen > NODE_BUDGET {
                    break;
                }
                if let Ok(next) = self.walker.GetNextSiblingElement(&el) {
                    stack.push((next, depth));
                }
                if !visit(&el, depth) {
                    break;
                }
                if depth < MAX_DEPTH {
                    if let Ok(child) = self.walker.GetFirstChildElement(&el) {
                        stack.push((child, depth + 1));
                    }
                }
            }
        }
    }

    fn find(&self, sel: &UiSelector, max: usize) -> Result<Vec<(UiElement, IUIAutomationElement)>> {
        let root = self.root(sel.window.as_deref())?;
        let mut found = Vec::new();
        self.walk(&root, |el, _| {
            let info = self.read(el);
            if matches(sel, &info) {
                found.push((info, el.clone()));
            }
            found.len() < max.max(1) * 4
        });
        // Rank by the element data, keep the matching live handles aligned.
        let ranked = rank(sel, found.iter().map(|(i, _)| i.clone()).collect());
        let mut out = Vec::new();
        for info in ranked.into_iter().take(max.max(1)) {
            if let Some((_, handle)) = found.iter().find(|(i, _)| i == &info) {
                out.push((info, handle.clone()));
            }
        }
        Ok(out)
    }
}

fn click_centre(desktop: &WindowsDesktop, rect: &Rect) -> Result<()> {
    use crate::types::Desktop;
    let x = rect.x + (rect.w / 2) as i32;
    let y = rect.y + (rect.h / 2) as i32;
    desktop.send(InputEvent::MoveTo { x, y })?;
    desktop.send(InputEvent::Button {
        button: MouseButton::Left,
        down: true,
    })?;
    desktop.send(InputEvent::Button {
        button: MouseButton::Left,
        down: false,
    })
}

pub fn find(sel: &UiSelector, max: usize) -> Result<Vec<UiElement>> {
    Ok(Session::new()?
        .find(sel, max)?
        .into_iter()
        .map(|(i, _)| i)
        .collect())
}

pub fn invoke(desktop: &WindowsDesktop, sel: &UiSelector) -> Result<String> {
    let session = Session::new()?;
    let (info, el) = session
        .find(sel, 1)?
        .into_iter()
        .next()
        .ok_or_else(|| OsError::InvalidArgument("no matching control was found".into()))?;
    if !info.enabled {
        return Err(OsError::InvalidArgument(format!(
            "“{}” is disabled",
            info.name
        )));
    }
    // SAFETY: pattern calls on a valid element.
    unsafe {
        if let Ok(p) = el.GetCurrentPatternAs::<IUIAutomationInvokePattern>(UIA_InvokePatternId) {
            if p.Invoke().is_ok() {
                return Ok(format!("Invoked “{}”", info.name));
            }
        }
        if let Ok(p) = el.GetCurrentPatternAs::<IUIAutomationTogglePattern>(UIA_TogglePatternId) {
            if p.Toggle().is_ok() {
                return Ok(format!("Toggled “{}”", info.name));
            }
        }
        if let Ok(p) =
            el.GetCurrentPatternAs::<IUIAutomationSelectionItemPattern>(UIA_SelectionItemPatternId)
        {
            if p.Select().is_ok() {
                return Ok(format!("Selected “{}”", info.name));
            }
        }
        if let Ok(p) = el
            .GetCurrentPatternAs::<IUIAutomationExpandCollapsePattern>(UIA_ExpandCollapsePatternId)
        {
            if p.Expand().is_ok() {
                return Ok(format!("Expanded “{}”", info.name));
            }
        }
    }
    if info.rect.w == 0 || info.rect.h == 0 {
        return Err(OsError::Platform(format!(
            "“{}” has no clickable area",
            info.name
        )));
    }
    click_centre(desktop, &info.rect)?;
    Ok(format!("Clicked “{}”", info.name))
}

pub fn set_value(desktop: &WindowsDesktop, sel: &UiSelector, text: &str) -> Result<String> {
    use crate::types::Desktop;
    let session = Session::new()?;
    let (info, el) = session
        .find(sel, 1)?
        .into_iter()
        .next()
        .ok_or_else(|| OsError::InvalidArgument("no matching control was found".into()))?;
    // SAFETY: pattern calls on a valid element.
    unsafe {
        if let Ok(p) = el.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId) {
            if p.SetValue(&BSTR::from(text)).is_ok() {
                return Ok(format!("Set “{}”", info.name));
            }
        }
        // Fallback: focus the control, select its content and type over it.
        let _ = el.SetFocus();
    }
    click_centre(desktop, &info.rect)?;
    for (vk, down) in [(0x11, true), (0x41, true), (0x41, false), (0x11, false)] {
        desktop.send(InputEvent::Vk {
            vk,
            extended: false,
            down,
        })?;
    }
    for unit in text.encode_utf16() {
        desktop.send(InputEvent::Unicode { unit, down: true })?;
        desktop.send(InputEvent::Unicode { unit, down: false })?;
    }
    Ok(format!("Typed into “{}”", info.name))
}

pub fn outline(window: Option<&str>, max_nodes: usize) -> Result<String> {
    let session = Session::new()?;
    let root = session.root(window)?;
    let mut lines = Vec::new();
    session.walk(&root, |el, depth| {
        let info = session.read(el);
        // Skip nameless layout containers to keep the outline short.
        if !info.name.trim().is_empty() || !info.automation_id.is_empty() {
            lines.push(outline_line(depth, &info));
        }
        lines.len() < max_nodes.max(1)
    });
    Ok(lines.join("\n"))
}
