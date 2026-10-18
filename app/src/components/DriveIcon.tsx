import { HardDrive, MemoryStick, Usb, Zap, Server } from "lucide-react";
import type { DriveKind } from "../lib/types";

export function DriveIcon({ kind, size = 26 }: { kind: DriveKind; size?: number }) {
  const props = { size, "aria-hidden": true, strokeWidth: 1.8 } as const;
  switch (kind) {
    case "hdd": return <HardDrive {...props} />;
    case "nvme": return <Zap {...props} />;
    case "ssd": return <Server {...props} />;
    case "usb": return <Usb {...props} />;
    case "sd": return <MemoryStick {...props} />;
    default: return <HardDrive {...props} />;
  }
}
