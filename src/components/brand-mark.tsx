/**
 * Compact application mark used in navigation and app chrome.
 *
 * The mark keeps the visual density low so it stays crisp at small sizes,
 * similar to modern desktop app icons that use a framed shell plus a simple
 * monochrome symbol.
 */

interface BrandMarkProps {
  className?: string;
}

export function BrandMark({ className = "h-12 w-12" }: BrandMarkProps) {
  return (
    <div
      className={`relative isolate flex items-center justify-center overflow-hidden rounded-[18px] border border-white/10 shadow-[0_16px_40px_rgba(0,0,0,0.38)] ${className}`}
      aria-hidden="true"
    >
      <div className="absolute inset-0 bg-[linear-gradient(160deg,#1a2230_0%,#0d1219_52%,#090c12_100%)]" />
      <div className="absolute inset-[1px] rounded-[17px] bg-[radial-gradient(circle_at_24%_18%,rgba(110,231,183,0.18),transparent_30%),radial-gradient(circle_at_82%_78%,rgba(56,189,248,0.18),transparent_36%),linear-gradient(180deg,rgba(255,255,255,0.08),rgba(255,255,255,0.01))]" />
      <div className="absolute inset-[6px] rounded-[12px] border border-white/8 bg-[#0d1218]/92 shadow-[inset_0_1px_0_rgba(255,255,255,0.06)]" />
      <div className="absolute inset-x-3 top-2 h-4 rounded-full bg-white/8 blur-md" />

      <svg
        viewBox="0 0 64 64"
        className="relative h-7 w-7 text-slate-50 drop-shadow-[0_4px_12px_rgba(255,255,255,0.15)]"
        fill="none"
        stroke="currentColor"
        strokeWidth="2.8"
        strokeLinecap="round"
        strokeLinejoin="round"
      >
        <path d="M18 24.5c4.2-2.7 8.8-4 14-4 5.3 0 9.9 1.3 14 4v18.7c-4.2-2.2-8.8-3.3-14-3.3-5.2 0-9.8 1.1-14 3.3V24.5Z" />
        <path d="M32 20.5v19.8" />
        <path d="m22.5 18 4.8-6.2 5.5 7.5" />
        <path d="m41.5 18-4.8-6.2-5.5 7.5" />
        <path d="M23.8 30.1c2.4-.9 4.8-1.4 7.3-1.5" opacity="0.72" />
        <path d="M40.2 30.1c-2.4-.9-4.8-1.4-7.3-1.5" opacity="0.72" />
      </svg>
    </div>
  );
}
