// page.desktop.tsx
// Home page displaying all tool categories in a card grid layout.

import { Sparkles } from "lucide-react";
import { CategoryCard } from "@/components/common/CategoryCard";
import { Badge } from "@/components/ui/feedback/Badge";
import { toolsByCategory } from "@/config/tools";
import { inDevelopmentSlugs } from "@/data/inDevelopmentSlugs";

export default function Home() {
  return (
    <div className="page-shell">
      <div className="page-grid-overlay" />

      <div
        aria-hidden="true"
        className="pointer-events-none fixed inset-0 overflow-hidden z-0"
      >
        <div className="absolute -top-32 -left-24 h-[480px] w-[480px] rounded-full bg-blue-500/10 blur-[120px]" />
        <div className="absolute -top-20 right-0 h-[360px] w-[360px] rounded-full bg-pink-500/10 blur-[100px]" />
        <div className="absolute top-[40%] -left-20 h-[280px] w-[280px] rounded-full bg-yellow-400/8 blur-[100px]" />
        <div className="absolute top-[35%] -right-20 h-[320px] w-[320px] rounded-full bg-green-400/8 blur-[110px]" />
        <div className="absolute bottom-0 left-1/2 -translate-x-1/2 h-[300px] w-[600px] rounded-full bg-red-500/6 blur-[120px]" />
      </div>

      <main className="page-main flex gap-10 flex-col items-center relative z-10">
        <section className="relative flex flex-col items-center overflow-visible rounded-2xl p-8 sm:p-10">
          <Badge
            variant="outline"
            className="flex mb-3 rounded-full gap-2 items-center badge-emerald px-4 py-2"
          >
            <Sparkles className="size-3.5 text-blue-400" />
            Runs on your PC
          </Badge>

          <div className="relative flex flex-col gap-5 items-center">
            <div
              aria-hidden="true"
              className="pointer-events-none absolute left-1/2 top-[44%] h-28 w-56 -translate-x-1/2 -translate-y-1/2 rounded-full bg-white/15 blur-[100px] sm:h-40 sm:w-[26rem]"
            />
            <h1 className="brand-title relative z-10 text-4xl font-semibold tracking-tight sm:text-8xl">
              Open Tools
            </h1>
            <p className="relative z-10 mt-4 max-w-2xl text-center text-base leading-7 text-muted-foreground sm:text-lg">
              A growing collection of focused utilities for image, video, audio,
              PDF and design work. Everything runs on your PC.
            </p>
          </div>
        </section>

        <section className="w-full">
          <div className="grid w-full grid-cols-1 gap-5 md:grid-cols-2 2xl:grid-cols-3">
            {toolsByCategory.map((category) => (
              <CategoryCard
                key={category.name}
                category={category}
                tools={category.tools}
                inDevelopmentSlugs={inDevelopmentSlugs}
              />
            ))}
          </div>
        </section>
      </main>
    </div>
  );
}
