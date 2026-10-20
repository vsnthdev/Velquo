import { motion } from "motion/react";

export function PairDevicePage() {
  return (
    <motion.div
      key="pair-device-page"
      initial={{ opacity: 0, y: 8 }}
      animate={{ opacity: 1, y: 0 }}
      exit={{ opacity: 0, y: -6 }}
      transition={{ type: "spring", stiffness: 460, damping: 34 }}
      className="relative z-10 flex flex-col"
    />
  );
}
