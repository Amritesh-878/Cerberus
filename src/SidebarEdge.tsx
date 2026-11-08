import { type EdgeLimits, ResizeEdge } from "./ResizeEdge";
import {
  clampPanel,
  clampSidebar,
  clampSplit,
  PANEL_DEFAULT,
  PANEL_MIN,
  panelMax,
  SIDEBAR_DEFAULT,
  SIDEBAR_MIN,
  sidebarMax,
  SPLIT_DEFAULT,
  SPLIT_MAX,
  SPLIT_MIN,
} from "./widths";

const SIDEBAR: EdgeLimits = {
  min: SIDEBAR_MIN,
  max: sidebarMax,
  clamp: clampSidebar,
  reset: SIDEBAR_DEFAULT,
};

const PANEL: EdgeLimits = {
  min: PANEL_MIN,
  max: panelMax,
  clamp: clampPanel,
  reset: PANEL_DEFAULT,
};

interface EdgeProps {
  width: number;
  onWidth: (width: number) => void;
  onDone: () => void;
}

export function SidebarEdge(props: EdgeProps) {
  return (
    <ResizeEdge
      class="sidebar-edge"
      label="Sidebar width"
      toWidth={(x) => x}
      step={16}
      limits={SIDEBAR}
      width={props.width}
      onWidth={(width) => {
        props.onWidth(width);
      }}
      onDone={() => {
        props.onDone();
      }}
    />
  );
}

export function PanelEdge(props: EdgeProps) {
  return (
    <ResizeEdge
      class="panel-edge"
      label="Panel width"
      // The panel sits on the right, so it grows as the pointer moves left.
      toWidth={(x) => innerWidth - x}
      step={-16}
      limits={PANEL}
      width={props.width}
      onWidth={(width) => {
        props.onWidth(width);
      }}
      onDone={() => {
        props.onDone();
      }}
    />
  );
}

const SPLIT: EdgeLimits = {
  min: SPLIT_MIN,
  max: () => SPLIT_MAX,
  clamp: clampSplit,
  reset: SPLIT_DEFAULT,
};

/** The divider between two chats side by side; its value is the left chat's share in thousandths. */
export function SplitEdge(
  props: EdgeProps & { area: () => DOMRect | undefined },
) {
  return (
    <ResizeEdge
      class="split-edge"
      label="Split between the two chats"
      toWidth={(x) => {
        const area = props.area();
        return area && area.width > 0
          ? ((x - area.left) / area.width) * 1000
          : props.width;
      }}
      step={20}
      limits={SPLIT}
      width={props.width}
      onWidth={(width) => {
        props.onWidth(width);
      }}
      onDone={() => {
        props.onDone();
      }}
    />
  );
}
