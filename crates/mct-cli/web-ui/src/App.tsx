import { LazyMotion, MotionConfig, domAnimation } from "motion/react";
import { Toaster } from "sonner";
import { DeadWorkspace } from "./DeadWorkspace";
import { DuplicateWorkspace } from "./DuplicateWorkspace";
import { useTheme } from "./hooks";
import type { WorkspaceProps } from "./types";

/** The open finding. Motion is short and follows the system's reduced-motion setting. */
export function App(props: WorkspaceProps) {
  const theme = useTheme();
  const { item } = props;
  return (
    <MotionConfig reducedMotion="user">
      <LazyMotion features={domAnimation} strict>
        {item.type === "duplicate" ? (
          <DuplicateWorkspace {...props} item={item} />
        ) : (
          <DeadWorkspace {...props} item={item} />
        )}
        <Toaster theme={theme} position="bottom-right" closeButton containerAriaLabel={props.bridge.t("rw.notifications")} />
      </LazyMotion>
    </MotionConfig>
  );
}
