import { BRAND } from '@/lib/brand.generated';
import React from "react";
import { Info as InfoIcon } from "lucide-react";
import { Dialog, DialogContent, DialogTitle, DialogTrigger } from "./ui/dialog";
import { VisuallyHidden } from "./ui/visually-hidden";
import { About } from "./About";

interface InfoProps {
    isCollapsed: boolean;
}

const Info = React.forwardRef<HTMLButtonElement, InfoProps>(({ isCollapsed }, ref) => {
  return (
    <Dialog aria-describedby={undefined}>
      <DialogTrigger asChild>
        <button 
          ref={ref} 
          className={`flex items-center cursor-pointer border-none transition-colors ${
            isCollapsed
              ? "justify-center bg-transparent p-2 hover:bg-surface-hover rounded-control"
              : "w-full gap-2 h-8 px-2 rounded-control text-sm text-ink-muted hover:bg-surface-hover hover:text-ink"
          }`}
          title={`About ${BRAND.name}`}
          aria-label={`About ${BRAND.name}`}
        >
          <InfoIcon className={isCollapsed ? "w-5 h-5 text-ink-muted" : "w-4 h-4"} />
          {!isCollapsed && <span>About</span>}
        </button>
      </DialogTrigger>
      <DialogContent>
        <VisuallyHidden>
          <DialogTitle>About {BRAND.name}</DialogTitle>
        </VisuallyHidden>
        <About />
      </DialogContent>
    </Dialog>
  );
});

Info.displayName = "About";

export default Info; 