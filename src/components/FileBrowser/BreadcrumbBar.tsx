import { useStore } from "../../store";

export function BreadcrumbBar() {
  const currentPath = useStore((s) => s.currentPath);
  const navigate = useStore((s) => s.navigate);

  const parts = currentPath.split("/").filter(Boolean);

  const navigateTo = (index: number) => {
    const path = "/" + parts.slice(0, index + 1).join("/");
    navigate(path);
  };

  return (
    <div className="flex items-center gap-1 px-3 py-2 text-sm text-gray-300 border-b border-gray-700">
      <button
        onClick={() => navigate("/")}
        className="hover:text-white text-gray-400"
      >
        ~
      </button>
      {parts.map((part, i) => (
        <span key={i} className="flex items-center gap-1">
          <span className="text-gray-600">/</span>
          <button
            onClick={() => navigateTo(i)}
            className={`hover:text-white ${i === parts.length - 1 ? "text-white" : "text-gray-400"}`}
          >
            {part}
          </button>
        </span>
      ))}
    </div>
  );
}
