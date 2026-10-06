export function HitMark({ className }: { className?: string }) {
  return <svg aria-hidden="true" className={className} viewBox="0 0 64 64">
    <circle cx="32" cy="32" r="19.5" fill="none" stroke="currentColor" strokeWidth="9" />
    <path d="M15.5 32H22.5M41.5 32H48.5M32 15.5V22.5M32 41.5V48.5" fill="none" stroke="currentColor" strokeWidth="9" strokeLinecap="round" />
  </svg>
}
