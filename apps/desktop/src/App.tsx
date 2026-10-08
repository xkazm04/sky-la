import { type CoreKind, commands } from "@skyla/ipc";
import { useEffect } from "react";
import { useQuery } from "./data";
import { Gallery } from "./gallery/Gallery";
import { navigate, type Screen, useRoute } from "./router";
import { AdvisorsScreen } from "./screens/Advisors";
import { BankScreen } from "./screens/Bank";
import { InboxScreen } from "./screens/Inbox";
import { InvoicesScreen } from "./screens/Invoices";
import { OverviewScreen } from "./screens/Overview";
import { PurchasesScreen } from "./screens/Purchases";
import { RegisterScreen } from "./screens/Register";
import { RecoverScreen, SetupScreen, sessionRoute, UnlockScreen } from "./screens/Session";
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
    case "purchases":
      return <PurchasesScreen item={item} />;
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
  // In the desktop app, books may not be open yet: set up or unlock first.
  const session = useQuery("session_state", () => commands.sessionState());
  const needed = session.state === "ready" ? sessionRoute(session.data) : null;
  const before =
    route.screen === "setup" || route.screen === "unlock" || route.screen === "recover";
  useEffect(() => {
    if (core === "tauri" && needed && !before) navigate(needed, null, true);
  }, [core, needed, before]);
  if (route.screen === "gallery") return <Gallery />;
  if (route.screen === "setup") return <SetupScreen />;
  if (route.screen === "unlock") return <UnlockScreen />;
  if (route.screen === "recover") return <RecoverScreen />;
  if (core === "tauri" && (session.state !== "ready" || needed)) return null;
  return (
    <Shell core={core} screen={route.screen}>
      <ScreenFor screen={route.screen} item={route.item} core={core} />
    </Shell>
  );
}
