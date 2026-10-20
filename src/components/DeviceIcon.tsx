import {
  Desktop,
  DeviceMobile,
  DeviceTablet,
  Laptop,
  Television,
} from "@phosphor-icons/react";

export function DeviceIcon({
  type,
  size = 18,
}: {
  type: string;
  size?: number;
}) {
  const normalized = type.toLowerCase();
  if (normalized === "phone" || normalized === "smartphone") {
    return <DeviceMobile size={size} weight="regular" className="shrink-0" />;
  }
  if (normalized === "laptop") {
    return <Laptop size={size} weight="regular" className="shrink-0" />;
  }
  if (normalized === "tablet") {
    return <DeviceTablet size={size} weight="regular" className="shrink-0" />;
  }
  if (normalized === "tv") {
    return <Television size={size} weight="regular" className="shrink-0" />;
  }
  return <Desktop size={size} weight="regular" className="shrink-0" />;
}
