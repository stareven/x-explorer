import { useStore } from "../../store";
import { TransferItem } from "./TransferItem";

export function TransferPanel() {
  const transfers = useStore((s) => s.transfers);

  const active = transfers.filter(
    (t) => t.status === "pending" || t.status === "running"
  );

  if (active.length === 0) return null;

  return (
    <div className="border-t border-gray-700 bg-gray-800 max-h-32 overflow-y-auto">
      {active.map((task) => (
        <TransferItem key={task.id} task={task} />
      ))}
    </div>
  );
}
