/**
 * sky-la design system: direction A, "Tahoe" (docs/design/DESIGN.md §6).
 * Tokens live in `tokens.css` (import `@skyla/ui/tokens.css` after Tailwind);
 * primitives are built on React Aria Components for keyboard and screen
 * reader behaviour.
 */

export {
  type Appearance,
  applyAppearance,
  type ResolvedAppearance,
  resolveAppearance,
  storeAppearance,
  storedAppearance,
} from "./appearance";
export { Badge, type BadgeProps, type Tone, toneClasses, toneIcon } from "./components/Badge";
export { Button, type ButtonProps, type ButtonVariant, buttonClasses } from "./components/Button";
export {
  Select,
  type SelectOption,
  type SelectProps,
  TextField,
  type TextFieldProps,
} from "./components/Field";
export {
  FactList,
  type FactListProps,
  Inspector,
  type InspectorProps,
  InspectorSection,
  type InspectorSectionProps,
} from "./components/Inspector";
export { isMac, Kbd, type KbdProps } from "./components/Kbd";
export {
  Menu,
  MenuItem,
  type MenuItemProps,
  type MenuProps,
  MenuSeparator,
} from "./components/Menu";
export { PopoverSurface, Popup, type PopupProps } from "./components/Popup";
export { SearchField, type SearchFieldProps } from "./components/SearchField";
export {
  type Segment,
  SegmentedControl,
  type SegmentedControlProps,
} from "./components/SegmentedControl";
export {
  type SourceItem,
  SourceList,
  type SourceListProps,
  type SourceSection,
} from "./components/SourceList";
export { StatusBar, type StatusBarProps, type StatusItem } from "./components/StatusBar";
export {
  DataTable,
  type DataTableProps,
  type TableColumn,
  type TableSection,
} from "./components/Table";
export {
  AppWindow,
  type AppWindowProps,
  ContentGroup,
  Toolbar,
  type ToolbarProps,
} from "./components/Window";
export { cx } from "./cx";
export { formatMinor } from "./format";
