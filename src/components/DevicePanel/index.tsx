import { DeviceList } from "./DeviceList";
import { AppList } from "./AppList";

export function DevicePanel() {
  return (
    <aside className="w-56 flex-shrink-0 bg-gray-800 border-r border-gray-700 flex flex-col overflow-y-auto">
      <DeviceList />
      <AppList />
    </aside>
  );
}
