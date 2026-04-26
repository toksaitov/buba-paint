export function Loading({ label }: { label?: string } = {}) {
  return (
    <div className="flex flex-col items-center justify-center gap-2 p-12">
      <div className="h-5 w-5 animate-spin border-2 border-border border-t-transparent rounded-full" />
      {label && <div className="text-[11px] text-muted">{label}</div>}
    </div>
  );
}
