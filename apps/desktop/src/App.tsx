import type { CoreKind } from "@skyla/ipc";
import { Gallery } from "./gallery/Gallery";
import { type Screen, useRoute } from "./router";
import { AdvisorsScreen } from "./screens/Advisors";
import { BankScreen } from "./screens/Bank";
import { InboxScreen } from "./screens/Inbox";
import { InvoicesScreen } from "./screens/Invoices";
import { OverviewScreen } from "./screens/Overview";
import { RegisterScreen } from "./screens/Register";
import { SettingsScreen } from "./screens/Settings";
import { StatementsScreen } from "./screens/Statements";
import { TaxesScreen } from "./screens/Taxes";
import { Shell } from "./shell/Shell";

function ScreenFor({
  screen,
  item,
  core,
}: {
  screen: Screen;
  item: string | null;
  core: CoreKind;
}) {
  switch (screen) {
    case "overview":
      return <OverviewScreen item={item} />;
    case "inbox":
      return <InboxScreen item={item} />;
    case "invoices":
      return <InvoicesScreen item={item} />;
    case "bank":
      return <BankScreen item={item} />;
    case "statements":
      return <StatementsScreen item={item} />;
    case "taxes":
      return <TaxesScreen item={item} />;
    case "advisors":
      return <AdvisorsScreen item={item} />;
    case "register":
      return <RegisterScreen item={item} />;
    case "settings":
      return <SettingsScreen item={item} core={core} />;
  }
}

/** The app: direction A's window with every v1 screen (WP-10); `#/gallery` shows the design system. */
export function App({ core }: { core: CoreKind }) {
  const route = useRoute();
  if (route.screen === "gallery") return <Gallery />;
  return (
    <Shell core={core} screen={route.screen}>
      <ScreenFor screen={route.screen} item={route.item} core={core} />
    </Shell>
  );
}
