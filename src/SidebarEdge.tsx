import { type EdgeLimits, ResizeEdge } from "./ResizeEdge";
import {
  clampPanel,
  clampSidebar,
  PANEL_DEFAULT,
  PANEL_MIN,
  panelMax,
  SIDEBAR_DEFAULT,
  SIDEBAR_MIN,
  sidebarMax,
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
      side="left"
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
      side="right"
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
