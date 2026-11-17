# Accessibility research: popover, menu, select/combobox, dialog, tooltip, tabs, switch, toast

Research note for Recap. The UI is Luau on zgx, a native Windows GUI framework. There is no DOM and no ARIA, so assistive technology (Narrator, NVDA, JAWS) sees the app only through **Windows UI Automation (UIA)**. ARIA is given below only because Base UI and the APG are written in ARIA terms. The UIA column is what Recap must actually expose.

Researched 2026-11-17. Sources:
- Base UI v1.9.0 docs (base-ui.com), one page per component, plus the overview page https://base-ui.com/react/overview/accessibility
- WAI-ARIA Authoring Practices Guide (APG), https://www.w3.org/WAI/ARIA/apg/patterns/
- Microsoft Learn, UIA docs:
  - Control types and their patterns: https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-controlpatternmapping
  - ARIA to UIA mapping: https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-ariaspecification. The role table is labelled "DRAFT" and is old (written for IE), but it is Microsoft's only published mapping.

How to read this note:
- "Base UI says" means the docs page states it. "Not stated" means the docs page does not say it. In that case the Base UI behaviour may still exist in code, but it was not verified from docs.
- Many Base UI keys are documented only indirectly, through the `reason` values of `onOpenChange` (`escape-key`, `outside-press`, `focus-out`, `list-navigation`, `item-press`, `trigger-hover`, `trigger-focus`). A reason existing means Base UI handles that interaction.
- Every Base UI `onOpenChange` can be vetoed with `eventDetails.cancel()`.

---

## 0. Shared facts used by every section

### 0.1 ARIA role to UIA control type (Microsoft mapping)

Source: https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-ariaspecification

| ARIA role | UIA control type | Notes |
|---|---|---|
| button | Button | |
| checkbox, menuitemcheckbox | CheckBox | |
| radio, menuitemradio | RadioButton | |
| combobox | ComboBox | |
| listbox | List | |
| option | ListItem | |
| menu | Menu | |
| menubar | MenuBar | |
| menuitem | MenuItem | |
| tablist | Tab | |
| tab | TabItem | |
| tabpanel | Pane | |
| dialog, alertdialog | Pane | `AriaRole` property keeps "dialog"/"alertdialog" |
| tooltip | ToolTip | |
| alert | Text | |
| status | StatusBar | |
| log, group, region | Group / Group / Pane | |
| separator | Separator | |
| switch | (no row) | See 0.3 |

ARIA states to UIA properties (same page):

| ARIA | UIA |
|---|---|
| aria-checked, aria-pressed | `ToggleState` (Toggle pattern) |
| aria-expanded | `ExpandCollapseState` (ExpandCollapse pattern) |
| aria-selected | `IsSelected` (SelectionItem pattern) |
| aria-disabled | `IsEnabled` |
| aria-labelledby | `LabeledBy` |
| aria-describedby | `DescribedBy` |
| aria-controls | `ControllerFor` |
| aria-activedescendant | `HasKeyboardFocus` on the active child. "expected to be represented as the element with focus in the accessibility object model." |
| aria-multiselectable | `CanSelectMultiple` (Selection pattern) |
| aria-required | `IsRequiredForForm` |
| aria-readonly | `IsReadOnly` |
| aria-hidden | `IsOffscreen` |
| aria-posinset / aria-setsize | Tree structure (position and child count) |
| aria-haspopup, aria-live, aria-atomic | No native UIA property. Only via the `AriaProperties` string. |

Important for zgx: UIA has **no active-descendant concept**. A native UIA provider moves focus by raising `UIA_AutomationFocusChangedEventId` on the item that is "focused" and returning `HasKeyboardFocus = true` for it. Inside zgx, the item can be "virtually" focused (input focus stays on a text box), but UIA must see it as focused.

### 0.2 Control types and required patterns

Source: https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-controlpatternmapping

| Control type | Must support | Conditional |
|---|---|---|
| Button | none | ExpandCollapse, Invoke, Toggle, Value |
| CheckBox | Toggle | none |
| ComboBox | ExpandCollapse | Selection, Value |
| List | none | Grid, MultipleView, Scroll, Selection |
| ListItem | SelectionItem | ExpandCollapse, Invoke, ScrollItem, Toggle, Value, ... |
| Menu | none | none |
| MenuBar | none | Dock, ExpandCollapse, Transform |
| MenuItem | none | ExpandCollapse, Invoke, SelectionItem, Toggle |
| RadioButton | SelectionItem | (never Toggle) |
| Tab | Selection | Scroll |
| TabItem | SelectionItem | (never Invoke) |
| ToolTip | none | Text, Window |
| Pane | none | Dock, Scroll, Transform (not Window) |
| Window | Transform, Window | Dock |

Button rule (https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-supportbuttoncontroltype): "All buttons should support the Invoke control pattern or the Toggle control pattern but not both." A button that opens a menu or a popup may support ExpandCollapse. `HelpText` "is typically the same type of information presented through a tooltip." Image-only buttons must have a `Name`: "Whenever an image is used to label a button, alternate text must be supplied for the button's Name property."

### 0.3 Switch has no ARIA mapping row

Microsoft's own WinUI `ToggleSwitch` reports control type **Button** with the **Toggle** pattern (`ToggleSwitchAutomationPeer`: `GetAutomationControlType` returns `Button`, `GetPattern` returns `Toggle`, with its own `LocalizedControlType`). Source: https://learn.microsoft.com/en-us/uwp/api/windows.ui.xaml.automation.peers.toggleswitchautomationpeer. Use that for Recap.

### 0.4 Live announcements in UIA

- **Live region**: the element sets `UIA_LiveSettingPropertyId` (30135) to `Off` (0), `Polite` (1) or `Assertive` (2), and the provider raises `UIA_LiveRegionChangedEventId` (20024) after its text changes. Windows 8 and later. Polite means AT should "not interrupt the user". Assertive means AT should "interrupt the user". Sources: https://learn.microsoft.com/en-us/windows/win32/api/uiautomationcore/ne-uiautomationcore-livesetting, https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-event-ids
- **Notification event**: `UiaRaiseNotificationEvent(provider, NotificationKind, NotificationProcessing, displayString, activityId)`. It needs no visible element text. Windows 10 1709 and later. Remark: if the window uses `WS_POPUP`, it "must also implement the Window Control Pattern and handle the WM_GETOBJECT message." Source: https://learn.microsoft.com/en-us/windows/win32/api/uiautomationcoreapi/nf-uiautomationcoreapi-uiaraisenotificationevent

`NotificationKind` (https://learn.microsoft.com/en-us/windows/win32/api/uiautomationcore/ne-uiautomationcore-notificationkind):

| Value | Meaning |
|---|---|
| ItemAdded (0) | Something was added that should be presented |
| ItemRemoved (1) | Something was removed |
| ActionCompleted (2) | An action completed |
| ActionAborted (3) | An action was aborted |
| Other (4) | Anything else |

`NotificationProcessing` (https://learn.microsoft.com/en-us/windows/win32/api/uiautomationcore/ne-uiautomationcore-notificationprocessing):

| Value | Meaning |
|---|---|
| ImportantAll (0) | ASAP, deliver all. Warning: can flood the user. |
| ImportantMostRecent (1) | ASAP, newest replaces older ones |
| All (2) | When possible, deliver all |
| MostRecent (3) | When possible, newest replaces older ones |
| CurrentThenMostRecent (4) | Do not interrupt the current one; then read only the newest pending one |
| ImportantCurrentThenMostRecent (5) | Same, but ASAP. Windows build 26100+ only |

`activityId` is a non-localized string that groups notifications. Use one id per toast kind, so `MostRecent` can replace older messages of the same kind.

### 0.5 Focus models (APG)

Source: https://www.w3.org/WAI/ARIA/apg/practices/keyboard-interface/

- **Roving tabindex**: only one item of a composite is in the Tab order. Arrow keys move real focus to the next item and make it the new Tab stop. Advantage: "the user agent will scroll the newly focused element into view."
- **Active descendant**: real focus stays on the container (or a text input). An attribute names the "active" child. The author must draw the focus ring and scroll the item into view.
- Pointer clicks must update the roving item or the active descendant too. Otherwise keyboard and mouse get out of sync.
- **Disabled items**: in menus, "disabled items are focusable when navigating through a menu with the arrow keys". The same applies to listbox options, tabs and tree items. They stay focusable but cannot be activated. A plain disabled button can be removed from the Tab order.
- **Shortcuts** supplement navigation and never replace it. Avoid OS keys (modifier + Tab/Enter/Space/Esc, Win + key, Alt + F-key) and screen reader keys (Caps Lock, Insert, Scroll Lock).

Base UI states that it follows the APG for keyboard navigation and focus management. The developer is responsible for the visible focus ring, contrast and accessible names (https://base-ui.com/react/overview/accessibility).

---

## 1. Popover

Sources: https://base-ui.com/react/components/popover, https://www.w3.org/WAI/ARIA/apg/patterns/dialog-modal/ (the APG has no "popover" pattern; a popover with interactive content is a **non-modal dialog**, see APG tooltip page: "If the popup needs focusable content, use a non-modal dialog instead.")

### Roles and states

| Part | ARIA | UIA |
|---|---|---|
| Trigger | `button`, `aria-expanded`, `aria-haspopup="dialog"`, `aria-controls` | Button + **ExpandCollapse** (expanded while open) |
| Popup | `dialog` (non-modal), labelled by Title, described by Description | Pane (AriaRole "dialog"), `Name` = title text, `LabeledBy` = title, `DescribedBy` = description |
| Title / Description | `h2` / `p` in Base UI | Text |

Base UI renders Trigger and Close as `<button>`, Title as `<h2>` ("A heading that labels the popover") and Description as `<p>`. The exact ARIA it emits is not stated on the page.

### Keyboard

| Key | Behaviour |
|---|---|
| Enter / Space on trigger | Opens (Base UI reason `trigger-press` can be a KeyboardEvent) |
| Tab / Shift+Tab | Moves through the popup's content. Non-modal: Tab past the end leaves the popup. Base UI closes on `focus-out` |
| Esc | Closes, focus returns to trigger (`escape-key`) |

### Focus

| | Base UI default |
|---|---|
| On open | First tabbable element in the popup. When opened by **touch**, the popup itself gets focus "to avoid opening the virtual keyboard". |
| Trapped | Not by default (`modal={false}`). See `modal` below. |
| On close | Trigger, or the element that had focus before. |

`initialFocus` / `finalFocus` accept: `false` (do not move focus), `true` (default), a ref (that element), or a function that gets the interaction type (`'mouse' | 'touch' | 'pen' | 'keyboard' | ''`) and returns an element, `true`/`null` (default), or `false`/`undefined` (do nothing).

### Pointer

- Outside press closes (`outside-press`).
- Focus leaving closes (`focus-out`).
- Optional hover-to-open on the Trigger.

### Base UI props that matter

| Prop | Default | Effect |
|---|---|---|
| `modal` (Root) | `false` | `true`: locks page scroll, blocks outside pointer input; focus is trapped **only if a `<Popover.Close>` is inside the Popup** (may be visually hidden). `'trap-focus'`: trap focus only, no scroll lock, outside pointer still works. |
| `openOnHover` (Trigger) | `false` | Hover also opens |
| `delay` (Trigger) | `300` ms | Hover open delay |
| `closeDelay` (Trigger) | `0` ms | Hover close delay |
| `initialFocus`, `finalFocus` (Popup) | default behaviour | see Focus |

Base UI vs APG:
- The APG does not cover popovers directly.
- Base UI's rule "render a Close button inside a trapping popup so touch screen readers can escape" matches the APG's "strongly recommended" visible close button for dialogs.
- Base UI's tooltip page: an info icon that shows content on hover is a **Popover with `openOnHover`**, not a tooltip. "If the trigger's purpose is to open the popup itself, it's a popover."

---

## 2. Menu (menu button, context menu, submenus)

Sources: https://base-ui.com/react/components/menu, https://base-ui.com/react/components/context-menu, https://www.w3.org/WAI/ARIA/apg/patterns/menubar/, https://www.w3.org/WAI/ARIA/apg/patterns/menu-button/, https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-supportmenucontroltype, https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-supportmenuitemcontroltype

### Roles and states

| Part | ARIA (APG) | UIA |
|---|---|---|
| Menu button | `button`, `aria-haspopup="menu"` (or `true`), `aria-expanded`, optional `aria-controls` | Button + **ExpandCollapse** |
| Menu popup | `menu`, `aria-labelledby` = the button (or `aria-label`), `aria-orientation` default vertical | **Menu**. No required patterns. Must raise `UIA_MenuOpenedEventId` when shown and `UIA_MenuClosedEventId` when hidden. |
| Item | `menuitem` | MenuItem + **Invoke** |
| Item with submenu | `menuitem`, `aria-haspopup="menu"`, `aria-expanded` | MenuItem + **ExpandCollapse** |
| Checkbox item | `menuitemcheckbox`, `aria-checked` | MenuItem + **Toggle** (ARIA map says CheckBox type; Microsoft's MenuItem page says MenuItem with Toggle. Prefer MenuItem + Toggle so the item stays in the menu's item set) |
| Radio item | `menuitemradio`, `aria-checked` | MenuItem + **SelectionItem** |
| Separator | `separator` (not focusable) | Separator |
| Disabled | `aria-disabled="true"`, still focusable | `IsEnabled = false`, still reachable |

UIA notes from Microsoft:
- Content view hides the Menu element: "Menu is absent from the UI Automation tree because it does not convey meaningful information".
- "Context menu controls are a special case. They may appear as children of the desktop or of a top level application window." (relevant if zgx shows the menu in its own popup HWND).
- Microsoft's Win32 menus always also expose Invoke on checkable items, so clients are not confused when Toggle appears.

### Keyboard: menu button (APG)

| Key (focus on button) | Behaviour |
|---|---|
| Enter | Open, focus first item |
| Space | Open, focus first item |
| Down Arrow (optional) | Open, focus first item |
| Up Arrow (optional) | Open, focus **last** item |

### Keyboard: open menu (APG)

| Key | Behaviour |
|---|---|
| Down / Up Arrow | Next / previous item. Wrap is optional |
| Home / End | First / last item |
| Enter | Activate item and close menu. On a submenu parent: open submenu, focus its first item |
| Space (optional) | Checkbox item: toggle **without closing**. Radio item: check it, uncheck the others, **keep open**. Other items: like Enter |
| Right Arrow | On a submenu parent: open it, focus its first child. Otherwise nothing in a menu opened from a menu button |
| Left Arrow | In a submenu: close it, focus returns to parent item |
| Esc | Close current menu, focus returns to opener (button or parent item) |
| Tab / Shift+Tab | Move focus out of the menu **and close all menus**. Tab never moves into a menu |
| Printable char (optional) | Typeahead: next item whose label starts with the char |

Disabled items are focusable but cannot be activated. Separators are skipped.

### Focus

- Menus are composite widgets. Tab does not move between items; arrows do.
- APG allows roving tabindex or active descendant. Base UI moves real focus to items (roving). Items get `data-highlighted`.
- On open: first item (from keyboard). Base UI: "Opening from the keyboard highlights the first item."
- On close: focus returns to the trigger. Base UI `finalFocus` default: trigger or previously focused element.
- Not a focus trap. Tab closes the menu and leaves.

### Context menu

| Topic | APG | Base UI |
|---|---|---|
| Open | Not a separate APG pattern. Windows convention is Shift+F10 / the Menu key. | Right click or long press, positioned at the pointer. Keyboard opening is **not stated**. Trigger is a `<div>`, not a button. |
| Advice | | "Treat the context menu as an enhancement." Users "may not discover or be able to open a context menu, especially on touch devices or with assistive technology." Always give visible controls for the same actions. |
| Keys inside | Same as menu | Same as Menu |

For Recap: support Shift+F10 and the Menu key on the focused element, and place the menu at the element when opened by keyboard. This is a Windows convention; the APG tabs pattern also uses Shift+F10 for a tab's popup menu.

### Pointer

- Outside press closes (`outside-press`).
- Base UI `modal={true}` (default) locks scroll and blocks outside pointer input while open.
- Submenus open on hover after a delay.
- Items highlight on hover (`highlightItemOnHover`, default `true`). Hover and keyboard share one highlight, so there is never a second "hover" highlight.

### Base UI props that matter

| Prop | Default | Effect |
|---|---|---|
| `modal` (Root) | `true` | Scroll lock + outside pointer blocked. "Nested menus ignore this prop, and menus opened by hover are never modal." |
| `openOnHover` (Trigger) | not listed (off) | Hover opens the root menu |
| `delay` / `closeDelay` (Trigger) | `100` / `0` ms | |
| SubmenuTrigger `openOnHover` | `true` | |
| SubmenuTrigger `delay` / `closeDelay` | `100` / `0` ms | |
| `loopFocus` | `true` | Wrap at ends |
| `orientation` | `'vertical'` | Which arrows move |
| `closeParentOnEsc` (SubmenuRoot) | `false` | Esc closes only the current submenu (matches APG) |
| `closeOnClick` | Item `true`; CheckboxItem, RadioItem, LinkItem `false` | Matches APG Space behaviour for check/radio |
| `label` (items) | text content | Overrides the typeahead text |
| `finalFocus` (Popup) | trigger | |

Filterable menu (Base UI): the popup becomes a `dialog` holding a `searchbox` and the `menu`. The input keeps focus and arrows move the highlight (active-descendant style). "Tab closes the menu, and Shift+Tab returns focus to the trigger." The input is focused only when opened by click or keyboard, not hover/touch.

Base UI vs APG: same keys. Base UI loops by default (APG: optional). Base UI adds `modal` (APG menus are not modal; they close on Tab and outside click).

---

## 3. Select and combobox

Sources: https://base-ui.com/react/components/select, https://base-ui.com/react/components/combobox, https://www.w3.org/WAI/ARIA/apg/patterns/combobox/, https://www.w3.org/WAI/ARIA/apg/patterns/combobox/examples/combobox-select-only/, https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-supportcomboboxcontroltype

A **select** is the APG "select-only combobox" (no text input). A **combobox** has an editable input that filters a list.

### Roles and states

| Part | ARIA (APG) | UIA |
|---|---|---|
| Trigger / input | `combobox`, `aria-expanded`, `aria-controls` = listbox, `aria-activedescendant` = active option, `aria-autocomplete` (`none`/`list`/`both`, editable only), label via `<label>`/`aria-labelledby`/`aria-label` | **ComboBox** + **ExpandCollapse** (required). **Value** if it accepts free text. **Selection** (may be delegated to the List) |
| Popup | `listbox` | **List** (+ Selection, + Scroll when it scrolls) |
| Option | `option`, `aria-selected="true"` on the current one | **ListItem** + **SelectionItem** |
| Group / group label | `group` + label | Group |

Microsoft ComboBox rules:
- Tree: ComboBox > Edit (0 or 1) + List > ListItems + Button (1).
- `Name` comes from the static label. "The Name property should never contain the current contents of the combo box or change when the contents of the combo box change." The current value goes in Value/Selection, never in Name.
- Raise `ExpandCollapseState` property-changed events, and `Value` property-changed if Value is supported.
- Scroll is never on the ComboBox itself, only on its List while visible.

### Keyboard: select-only (APG example)

Closed:

| Key | Behaviour |
|---|---|
| Enter, Space, Down, Alt+Down | Open. Visual focus on the current option |
| Up | Open, visual focus on the first option |
| Home / End | Open, focus first / last option |
| Printable chars | Open, focus first match. Fast typing matches the whole string. Repeating one char cycles through items starting with it |

Open:

| Key | Behaviour |
|---|---|
| Down / Up | Next / previous option. No wrap: stops at ends |
| Home / End | First / last option |
| PageUp / PageDown | Move 10 options, stop at ends |
| Enter, Space, Alt+Up | Choose focused option, close, focus back on combobox |
| Tab | Choose focused option, close, move to next focusable element |
| Esc | Close **without** changing the value |
| Printable chars | Typeahead, as when closed |

### Keyboard: editable combobox (APG)

| Key (in input) | Behaviour |
|---|---|
| Down | Open if needed, move into the list (first item, or the item after an auto-selected one) |
| Up (optional) | Move to last item |
| Alt+Down (optional) | Open without moving focus |
| Alt+Up (optional) | Close, focus back to input |
| Esc | Close. If already closed, may clear the input |
| Enter | Accept the highlighted suggestion |
| Printable chars | Type into the input |
| Left / Right, Home / End (list open) | Return to the input and move the caret |

Standard text-editing keys must keep working in the input. Do not intercept them.

### Focus

- APG: DOM focus **stays on the combobox** for listbox popups. The active option is shown by `aria-activedescendant`. Only the combobox is in the Tab order (not the popup, not the open button).
- In UIA terms: raise `AutomationFocusChanged` on the highlighted ListItem (it reports `HasKeyboardFocus`) while the zgx input focus stays on the trigger/input. See 0.1.
- On close: focus is on the trigger/input (it never left).

### Pointer

- Click on trigger toggles open.
- Click on an option chooses it and closes.
- Outside press closes (`outside-press`).
- Hover highlights (Base UI `highlightItemOnHover` default `true`).

### Base UI props that matter

Select:

| Prop | Default | Effect |
|---|---|---|
| `modal` | `true` | Scroll lock + outside pointer blocked |
| `alignItemWithTrigger` | `true` | Popup overlaps trigger so the selected item lines up with the value text. Mouse only; falls back near screen edges |
| `highlightItemOnHover` | `true` | |
| Item `label` | text | Typeahead text |
| `multiple` | `false` | |
| `finalFocus` | trigger | |

Label: Base UI `Select.Label` click "focuses the trigger without opening the popup". Without a visible label, put `aria-label` on the trigger. Base UI recommends Combobox instead of Select for large lists because Select cannot be filtered.

Combobox:

| Prop | Default | Effect |
|---|---|---|
| `modal` | `false` | |
| `openOnInputClick` | `true` | |
| `autoHighlight` | `false` | Highlight first match while typing |
| `loopFocus` | `true` | Arrows wrap **through the input**: "The input is always included in the focus loop per ARIA Authoring Practices." |
| `highlightItemOnHover` | `true` | |
| `grid` | `false` | Makes the list a grid |
| `Combobox.Status`, `Combobox.Empty` | | Polite announcements for async status and "no results". The element "must remain mounted ... to announce changes consistently". Change its children; do not hide or remove the element itself. |

Base UI vs APG:
- Base UI combobox wraps by default (APG select example stops at the ends).
- Base UI's Select is modal by default; the APG combobox is not modal.
- Base UI docs do not list Home/End, PageUp/PageDown or Alt+arrow keys for Select (not stated). Use the APG table for Recap.

---

## 4. Dialog and alert dialog

Sources: https://base-ui.com/react/components/dialog, https://base-ui.com/react/components/alert-dialog, https://www.w3.org/WAI/ARIA/apg/patterns/dialog-modal/, https://www.w3.org/WAI/ARIA/apg/patterns/alertdialog/, https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-implementingwindow

### Roles and states

| Part | ARIA (APG) | UIA |
|---|---|---|
| Dialog | `dialog`, `aria-modal="true"`, `aria-labelledby` = title (or `aria-label`), optional `aria-describedby` | Pane (AriaRole "dialog") inside the window. If the dialog is its **own HWND**: Window control type + **Window** pattern with `IsModal = true`, raise `Window_WindowOpened` / `Window_WindowClosed`. Microsoft lists "modal dialogs" among controls that must implement Window. |
| Alert dialog | `alertdialog`, `aria-modal="true"`, `aria-labelledby`, `aria-describedby` = **the alert message (required)** | Same, AriaRole "alertdialog". The separate role lets AT "distinguish alert dialogs from other dialogs", for example "playing a system alert sound". |
| Close button | `button` | Button + Invoke |

APG on `aria-modal`: mark a dialog modal only when both (1) code blocks all interaction outside it and (2) the visuals obscure the outside. A dialog marked modal that does not behave modally causes "severe negative ramifications" for AT users.

### Keyboard (APG modal dialog)

| Key | Behaviour |
|---|---|
| Tab | Next tabbable element inside; from the last, wrap to the first |
| Shift+Tab | Previous; from the first, wrap to the last |
| Esc | Close the dialog |

The alert dialog uses the same keys.

### Focus

On open (APG):
- Default: the first focusable element.
- Long or structured content (lists, tables, many paragraphs): focus a static element at the top (title or first paragraph) made focusable, so the start is read and not scrolled away. In that case omit `aria-describedby`.
- Destructive or irreversible action: focus the **least destructive** button (Cancel). This is the usual alert dialog case.
- Simple info/confirm dialog: focus the most used button (OK / Continue).

Trapped: yes. Content outside is inert. A modal dialog gives no way to move focus outside without closing.

On close (APG): return focus to the element that opened it, unless:
- that element no longer exists (then pick a logical element), or
- the dialog's task leads directly to the next step (example: "Add rows" closes and focus goes to the first new cell).

Always include a visible close button in the Tab order ("strongly recommended").

### Pointer

- Base UI: outside press closes a Dialog (`outside-press`), unless `disablePointerDismissal`.
- Backdrop makes background content inert.
- Alert Dialog: the page does not describe outside-click behaviour (not stated). An alert dialog "requires a user response to proceed", so outside click should **not** close it.

### Base UI props that matter

| Prop | Default | Effect |
|---|---|---|
| `modal` (Dialog Root) | `true` | Trap focus + scroll lock + outside pointer blocked. `'trap-focus'`: trap only. `false`: none. |
| `disablePointerDismissal` | `false` | `true`: outside press does not close. For non-modal dialogs it also stops closing on focus loss. |
| Esc | always requests close | Veto with `eventDetails.cancel()` |
| `initialFocus` | first tabbable; popup itself when opened by touch | |
| `finalFocus` | trigger or previously focused element | |
| `Backdrop forceRender` | `false` | Nested dialogs do not draw their own backdrop |
| Alert Dialog Root | no `modal` prop | Always modal |

Base UI guidance: "Always offer a visible close button in the corner". Put elements that look outside the popup inside `Dialog.Popup` so they stay in the Tab order. For nested dialogs, "only the topmost layer should feel interactive".

Base UI vs APG: Base UI's default initial focus is "first tabbable". The APG rule for destructive alert dialogs (focus the least destructive action) must be set by the app via `initialFocus`.

---

## 5. Tooltip

Sources: https://base-ui.com/react/components/tooltip, https://www.w3.org/WAI/ARIA/apg/patterns/tooltip/, https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-supporttooltipcontroltype

The APG tooltip pattern is marked "work in progress; it does not yet have task force consensus."

### Roles and states

| Part | ARIA | UIA |
|---|---|---|
| Tooltip | `tooltip`. It never takes focus | **ToolTip**. Raise `UIA_ToolTipOpenedEventId` when shown and `UIA_ToolTipClosedEventId` when hidden. `Name` = the tooltip text. |
| Trigger | `aria-describedby` = tooltip | The trigger's **HelpText** = tooltip text. Microsoft: a text-only tooltip "is available as the HelpText property from the control that raised it". |

UIA rules:
- A non-focusable tooltip is not in the content view. Its text must be on the owner's `HelpText`.
- Tooltip popups do not implement the Window pattern, unless they have a Close button ("balloon help").

### Tooltips on icon buttons vs a label

Base UI is clear:
- "Tooltips are visual-only elements and are not a replacement for labeling the trigger."
- The trigger "must have an `aria-label` attribute that closely matches the tooltip's content". The icon itself is `aria-hidden`.
- "Tooltips alone are not accessible to touch or screen reader users." "A user should not miss critical information if they never see a tooltip."

For UIA this means:
- **Icon-only button**: `Name` = the short label (for example "Show notes"). The tooltip shows the same words. If the tooltip adds a shortcut ("Ctrl+Shift+N"), put the shortcut in `AcceleratorKey`, not in `Name`.
- **Button with a visible text label**: a tooltip is usually not needed. If present, it carries extra description and maps to `HelpText` (APG `aria-describedby`).
- Essential content belongs inline or in a Popover, never only in a tooltip.

### Keyboard and focus

| Event | Behaviour |
|---|---|
| Trigger gets keyboard focus | Show (APG: tooltip appears on focus or hover; Base UI reason `trigger-focus`). Base UI skips the transition (`instant: 'focus'`). |
| Esc | Hide; focus stays on the trigger (APG: "Dismisses the Tooltip"; Base UI `escape-key`) |
| Trigger loses focus | Hide, if focus opened it (APG) |
| Focus | Stays on the trigger the whole time (APG) |

When to show on keyboard focus: the APG says yes, on focus. Base UI opens on focus. Practical rule for Recap: show on **keyboard** focus (focus that came from Tab/arrows), not on focus caused by a mouse click. The click case is covered by "hide on pointer down" below. This rule is inferred; the docs do not state focus-visible.

### Pointer

| Event | Behaviour |
|---|---|
| Hover | Show after a delay (APG: "typically appears after a small delay"; Base UI `delay` 600 ms) |
| Pointer leaves trigger and tooltip | Hide (APG: stays "as long as the cursor is over the trigger or the tooltip") |
| Hover over the tooltip itself | Stays open (Base UI `disableHoverablePopup` default `false`). This lets users with screen magnifiers read it. |
| Pointer down / click on trigger | Hide (Base UI `closeOnClick` default `true`) |
| Touch | Base UI: "tooltips are disabled on touch devices", since there is no discoverable way to show them before tapping. |

### Delay group (instant switching)

Base UI `Tooltip.Provider`: "once a tooltip becomes visible, the adjacent tooltips will be shown instantly." `timeout` (default **400 ms**): "Another tooltip will open instantly if the previous tooltip is closed within this timeout." Provider `delay`/`closeDelay` set shared delays (no defaults listed).

Effect: in a toolbar, the first tooltip waits 600 ms. Moving to the next button within 400 ms shows its tooltip at once, with no animation (`instant: 'delay'`).

### Base UI props that matter

| Prop | Default |
|---|---|
| Trigger `delay` | `600` ms |
| Trigger `closeDelay` | `0` ms |
| Trigger `closeOnClick` | `true` |
| Trigger `disabled` | `false` (only disables the tooltip, not the button) |
| Root `disableHoverablePopup` | `false` |
| Root `trackCursorAxis` | `'none'` |
| Provider `timeout` | `400` ms |

Base UI vs APG: the APG says "Tooltip widgets do not receive focus" and points to a non-modal dialog for interactive content. Base UI agrees: interactive or info-icon content is a Popover with `openOnHover`, and contextual feedback ("Copied") is an anchored Toast, which "ensures screen-reader announcement".

---

## 6. Tabs

Sources: https://base-ui.com/react/components/tabs, https://www.w3.org/WAI/ARIA/apg/patterns/tabs/, https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-supporttabitemcontroltype

### Roles and states

| Part | ARIA (APG) | UIA |
|---|---|---|
| Tab list | `tablist`, label via `aria-labelledby`/`aria-label`, `aria-orientation` (default horizontal) | **Tab** + **Selection** (required) |
| Tab | `tab`, `aria-selected` true/false, `aria-controls` = panel, optional `aria-haspopup` | **TabItem** + **SelectionItem** (required). **Never Invoke.** `ControllerFor` may point to the panel. Raise `SelectionItem_ElementSelected` / `ElementRemovedFromSelection`. |
| Panel | `tabpanel`, `aria-labelledby` = its tab | **Pane**, `LabeledBy` = its tab |

### Keyboard (APG)

| Key | Behaviour |
|---|---|
| Tab | Into the tab list: focus the **active** tab. Out of it: next element, usually the panel |
| Left / Right (horizontal) | Previous / next tab, **wrapping**. May also activate it |
| Up / Down (vertical) | Same, for vertical lists. A horizontal list ignores Up/Down so they still scroll |
| Home / End (optional) | First / last tab, may activate |
| Space / Enter | Activate the focused tab (manual activation) |
| Shift+F10 | Open the tab's popup menu, if any |
| Delete (optional) | Close a closable tab, focus the next (or previous) tab |

### Focus

- Roving tabindex: one Tab stop for the whole list, on the active tab.
- Panel: if it has no focusable content (or its first content is not focusable), make the panel itself focusable (APG `tabindex="0"`) so Tab reaches it.

### Automatic vs manual activation (APG)

- **Automatic** (selection follows focus) is recommended when panels show "without noticeable latency".
- **Manual** (Enter/Space) when panels are slow to show.

### Base UI props that matter

| Prop | Default | Note |
|---|---|---|
| `activateOnFocus` (List) | `false` | Base UI is **manual** by default; APG recommends automatic for fast panels |
| `loopFocus` (List) | `true` | Matches APG wrap |
| `orientation` (Root) | `'horizontal'` | |
| `defaultValue` | `0` | `null` means no active tab |
| Panel `keepMounted` | `false` | Hidden panels are unmounted |
| Tab `disabled` | | First enabled tab is selected if the first is disabled |

Not stated in Base UI docs: Home/End, panel focusability, whether disabled tabs are focusable. APG: disabled tabs should stay focusable.

---

## 7. Switch

Sources: https://base-ui.com/react/components/switch, https://www.w3.org/WAI/ARIA/apg/patterns/switch/, https://learn.microsoft.com/en-us/uwp/api/windows.ui.xaml.automation.peers.toggleswitchautomationpeer

### Roles and states

| Part | ARIA (APG) | UIA |
|---|---|---|
| Switch | `switch`, `aria-checked` true/false, name from inner text / `aria-labelledby` / `aria-label`, optional `aria-describedby` | **Button** + **Toggle** (as WinUI ToggleSwitch). `ToggleState` On/Off. Raise `ToggleToggleState` property-changed. Give it its own `LocalizedControlType` string, as the WinUI peer does. |
| Group of switches | `group` + `aria-labelledby`, or `fieldset`/`legend` | Group, `Name` = group label |

APG: "it is critical the label on a switch does not change when its state changes." Only the state changes. In UIA, `Name` stays fixed; `ToggleState` changes.

Read-only: UIA `IsReadOnly`. Base UI `readOnly`: the user cannot toggle.

### Keyboard (APG)

| Key | Behaviour |
|---|---|
| Space | Toggle |
| Enter (optional) | Toggle |

### Focus and pointer

- One Tab stop.
- Click on the switch or on its label toggles it. Base UI wraps the switch in a `<label>` so the label is clickable.

### Base UI props that matter

| Prop | Default |
|---|---|
| `defaultChecked` | `false` |
| `disabled` | `false` |
| `readOnly` | `false` |
| `required` | `false` |
| `nativeButton` | `false` (renders `<span>` + hidden `<input>`) |

Keyboard and role are not stated on the Base UI page. Base UI: "Form controls must have an accessible name."

---

## 8. Toast

Sources: https://base-ui.com/react/components/toast, https://www.w3.org/WAI/ARIA/apg/patterns/alert/, https://learn.microsoft.com/en-us/windows/win32/api/uiautomationcoreapi/nf-uiautomationcoreapi-uiaraisenotificationevent

The APG has no toast pattern. The nearest pattern is **alert** (role `alert`) for important messages. A polite status message is a `status` live region (ARIA mapping: StatusBar).

### Roles and states

| Part | ARIA | UIA |
|---|---|---|
| Viewport | Landmark region (Base UI: "landmark region") | Group/Pane with a `Name` (for example "Notifications") so F6 lands somewhere named |
| Toast | Low priority: polite. High priority: urgent (Base UI wording; exact `aria-live` values not stated) | Announce with `UiaRaiseNotificationEvent` (see below), or a live region (`LiveSetting` Polite/Assertive + `LiveRegionChanged`) |
| Title / Description | `h2` / `p` | Text |
| Action / Close | `button` | Button + Invoke, with a clear `Name` ("Undo", "Dismiss") |

### Announcing

Base UI:
- `priority: 'low'` (default): "announced politely".
- `priority: 'high'`: "announced urgently".
- "For high priority toasts, the `title` and `description` strings are what are used to announce the toast to screen readers." Other content inside the toast is not announced unless the user navigates to the viewport.

APG alert:
- Dynamically rendered alerts are announced by most screen readers.
- Alerts must "not affect keyboard focus".
- "avoid designing alerts that disappear automatically". Disappearing too fast can fail WCAG 2.2.3. Frequent interruptions hurt WCAG 2.2.4.

UIA mapping for Recap:

| Toast kind | Call |
|---|---|
| Normal info ("Copied", "Saved") | `UiaRaiseNotificationEvent(…, ActionCompleted, MostRecent, text, "recap.toast")` |
| Error / needs attention | `UiaRaiseNotificationEvent(…, Other or ActionAborted, ImportantMostRecent, text, "recap.toast.error")` |
| Item appeared in a list | `ItemAdded` |

Use `ImportantAll` rarely: Microsoft warns it can flood the user. `displayString` = title + description, the same text Base UI announces. Windows 10 1709 and later. The provider must be a real UIA element in an HWND that answers `WM_GETOBJECT`; a `WS_POPUP` window must also implement the Window pattern.

Fallback: a live-region element (`LiveSetting` + `LiveRegionChanged`, Windows 8+). Same rule as Base UI `Combobox.Status`: keep the element mounted and change its text.

### Keyboard

| Key | Behaviour (Base UI) |
|---|---|
| F6 | "lets users jump into the toast viewport landmark region to navigate toasts with keyboard focus" |
| F6 again | "returns focus to where it was" |
| Tab inside viewport | Moves between toast buttons (Action, Close) |
| Esc | Not stated on the page |

Toasts never take focus on their own (APG alert).

### Timing and pause

| Base UI | Default |
|---|---|
| `timeout` (Provider and per toast) | `5000` ms. `0` = never auto-dismiss |
| Re-add with the same id | Updates in place and **restarts** the timer |
| `limit` | `3`. Older toasts are kept mounted but `inert` (`data-limited`) |
| `swipeDirection` | `['down', 'right']` |
| Viewport `data-expanded` | Set while the viewport "is being hovered or has focus" |

Pausing on hover, focus or window blur is **not stated** on the Base UI page. `data-expanded` shows that Base UI tracks hover/focus on the viewport. For Recap, pause all toast timers while the pointer is over the viewport or focus is inside it. This follows from the APG / WCAG 2.2.3 note on alerts that disappear. Toasts with an action (Undo) should use a long timeout or `0`.

Base UI advice: use an anchored toast, not a tooltip, for feedback such as "Copied", because a toast is announced.

---

## 9. Cross-component rules (summary)

1. Every interactive element has a fixed, short `Name`. State lives in patterns (`ToggleState`, `ExpandCollapseState`, `IsSelected`, `Value`), never in the name.
2. A trigger that opens a popup (menu button, popover trigger, select) exposes **ExpandCollapse** and keeps it in sync. Menus raise MenuOpened/MenuClosed; tooltips raise ToolTipOpened/ToolTipClosed.
3. Esc closes the topmost layer only and returns focus to its opener (submenu to parent item, menu to button, dialog to trigger).
4. Composite widgets (menu, listbox, tab list) have one Tab stop. Arrows move inside. Home/End go to the ends. Typeahead finds items by label.
5. Lists where typing must continue (combobox) keep real focus in the input and report the highlighted item to UIA as focused.
6. Only modal dialogs and alert dialogs trap focus. Menus and popovers close on Tab-out, outside click or focus loss.
7. Initial focus: first tabbable by default; the least destructive button in a destructive alert dialog; a static top element for long content.
8. Disabled items in menus, lists and tabs stay focusable but do nothing when activated.
9. Tooltips never carry essential content and never take focus. Icon buttons must have their own `Name`; the tooltip text goes to `HelpText`.
10. Status messages are announced without moving focus. Toasts are reachable by a shortcut (F6) and must not vanish while the user hovers or focuses them.

---

## 10. What a small accessible component set needs from the framework

Each item is a requirement on zgx (Rust host + Luau). Where a primitive does not exist yet, it is a gap to fill before the component set can be accessible.

**UIA tree and patterns**
- [ ] The framework must expose every widget as a UIA element with `ControlType`, `Name`, `LocalizedControlType`, `AutomationId`, `BoundingRectangle`, `IsEnabled`, `IsKeyboardFocusable`, `HasKeyboardFocus`, `IsOffscreen`, `HelpText`, `AcceleratorKey`, `LabeledBy`, `DescribedBy` and `ControllerFor`, settable from Luau.
- [ ] The framework must let a Luau component declare its UIA patterns and back them with callbacks: **Invoke** (button, menu item), **Toggle** (switch, checkbox item), **ExpandCollapse** (menu button, popover trigger, select, submenu item), **Selection / SelectionItem** (tab list/tab, listbox/option, radio item), **Value** (editable combobox, text input), **Window** (dialog in its own HWND, with `IsModal`).
- [ ] The framework must raise property-changed events when a pattern state changes (`ToggleState`, `ExpandCollapseState`, `IsSelected`, `Value`, `Name`, `IsEnabled`) and structure-changed events when children are added or removed.
- [ ] The framework must raise `MenuOpened`/`MenuClosed`, `ToolTipOpened`/`ToolTipClosed` and `Window_WindowOpened`/`WindowClosed` when those layers show and hide.
- [ ] The framework must place popups (menus, listboxes, popovers, tooltips) in the UIA tree under their owner, also when they are drawn in a separate top-level HWND, and answer `WM_GETOBJECT` for every such HWND.

**Focus**
- [ ] **Focus scope / trap**: a scope that wraps Tab and Shift+Tab from last to first and back, and makes everything outside inert to keyboard, pointer and UIA. Modes: none, trap only, trap + block outside (Base UI `modal` = `false` / `'trap-focus'` / `true`).
- [ ] **Initial focus**: on open, focus the first tabbable element by default, or an element chosen by the component (ref or function of the open input type: mouse, touch, pen, keyboard).
- [ ] **Return focus**: remember the element focused before a layer opened and restore it on close, with an override (ref, "do not move", or function of the close input type). Handle the case where that element no longer exists.
- [ ] **Roving focus**: a helper for one Tab stop per group, arrows by orientation, Home/End, optional wrap, disabled items still reachable, and the last active item restored when Tab re-enters.
- [ ] **Virtual focus (active descendant)**: keep input focus in a text box while UIA sees a list item as focused (`HasKeyboardFocus` + `AutomationFocusChanged` on the item), with a visible highlight and scroll-into-view.
- [ ] **Focus-visible**: know whether focus came from keyboard or pointer, so focus rings and tooltips-on-focus appear only for keyboard focus.

**Dismiss layer**
- [ ] A layer stack where Esc closes only the topmost layer, an outside pointer-down closes it (optionally blocking the click from reaching what is underneath), and focus leaving the layer can close it. Each close carries a reason (`escape-key`, `outside-press`, `focus-out`, `item-press`) and can be cancelled.
- [ ] Nested layers (submenu in menu, dialog in dialog) must count as "inside" their parent for outside-click and focus-out.

**Text and pointer timing**
- [ ] **Typeahead**: a buffer that matches item labels by prefix, resets after a short pause, cycles on a repeated single character, and uses an override label per item.
- [ ] **Hover intent**: open and close delays per trigger, with cancel when the pointer leaves first, and a "safe" path so moving from trigger to popup (submenu, hoverable tooltip) does not close it.
- [ ] **Delay group**: a shared state so that, after one tooltip has shown, the next one in the group opens instantly if the previous closed within a timeout (Base UI 400 ms), and skips its animation.
- [ ] Tooltip close on Esc, on pointer down on the trigger, on focus loss and on pointer leave, with no focus change.

**Announcements**
- [ ] **Live announcements**: a Luau call that raises `UiaRaiseNotificationEvent` with kind, processing and activity id. Optional fallback: live-region elements (`LiveSetting` + `LiveRegionChanged`).
- [ ] Timers that pause while the pointer is over a region or focus is inside it, and a global shortcut (F6) that moves focus into a named region and back.

**Shortcuts and context menus**
- [ ] The Menu key and Shift+F10 must open a context menu for the focused element, placed at that element.
- [ ] Shortcut keys must not take OS or screen-reader combinations (modifier + Tab/Enter/Space/Esc, Win + key, Insert, Caps Lock).
