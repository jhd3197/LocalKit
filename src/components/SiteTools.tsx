import type { SiteDetail } from "../lib/types";
import DatabasePanel from "./DatabasePanel";
import SearchReplacePanel from "./SearchReplacePanel";
import DebugPanel from "./DebugPanel";
import ConfigEditorPanel from "./ConfigEditorPanel";

/**
 * The Tools tab on SiteDetail (plan 24): the inner-loop things a WordPress dev
 * would otherwise open an external app for — a database GUI, a search-replace,
 * WP_DEBUG + the debug log, and a config editor. Each section is capability-
 * gated, so the tab only shows what the site's kind actually supports.
 */
export default function SiteTools({
  detail,
  running,
  onShowSnapshots,
}: {
  detail: SiteDetail;
  running: boolean;
  onShowSnapshots?: () => void;
}) {
  const caps = detail.capabilities;
  return (
    <div className="mt-6 space-y-4">
      {caps.db_gui && <DatabasePanel siteId={detail.id} running={running} />}
      {caps.search_replace && (
        <SearchReplacePanel siteId={detail.id} running={running} onShowSnapshots={onShowSnapshots} />
      )}
      {caps.wp_tools && <DebugPanel siteId={detail.id} />}
      {caps.wp_tools && <ConfigEditorPanel siteId={detail.id} />}
    </div>
  );
}
