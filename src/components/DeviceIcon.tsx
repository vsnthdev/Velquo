import {
    LaptopIcon,
    DesktopIcon,
    TelevisionIcon,
    DeviceTabletIcon,
    DeviceMobileIcon,
} from "@phosphor-icons/react";

export function DeviceIcon({
    type,
    size = 18,
}: {
    type: string;
    size?: number;
}) {
    const normalized = type.toLowerCase();
    if (normalized == "phone" || normalized == "smartphone") {
        return <DeviceMobileIcon size={size} weight="regular" className="shrink-0" />;
    }
    if (normalized == "laptop") {
        return <LaptopIcon size={size} weight="regular" className="shrink-0" />;
    }
    if (normalized == "tablet") {
        return <DeviceTabletIcon size={size} weight="regular" className="shrink-0" />;
    }
    if (normalized == "tv") {
        return <TelevisionIcon size={size} weight="regular" className="shrink-0" />;
    }
    return <DesktopIcon size={size} weight="regular" className="shrink-0" />;
}
