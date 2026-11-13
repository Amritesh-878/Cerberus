import { For, type JSX, Show } from "solid-js";

import { SidebarIcon } from "./icons";
import { initials } from "./sidebarLayout";
import type { Project, ProjectId } from "./tree";

/** The collapsed sidebar: an expand button and one icon per project, with a dot where a chat waits on you. */
export function SidebarRail(props: {
  projects: readonly Project[];
  /** The project of the tab on screen. */
  active: ProjectId | undefined;
  waiting: (project: ProjectId) => boolean;
  /** Shown under the projects, such as the services' icon. */
  extra?: JSX.Element;
  onExpand: () => void;
  onOpenProject: (project: ProjectId) => void;
}) {
  return (
    <nav class="rail" aria-label="Projects">
      <button
        type="button"
        class="icon-button rail-expand"
        aria-label="Expand the sidebar"
        title="Expand the sidebar"
        onClick={() => {
          props.onExpand();
        }}
      >
        <SidebarIcon />
      </button>
      <ul class="rail-projects">
        <For each={props.projects}>
          {(project) => (
            <li classList={{ active: props.active === project.id }}>
              <button
                type="button"
                class="rail-project"
                classList={{ active: props.active === project.id }}
                aria-current={props.active === project.id ? "true" : undefined}
                aria-label={
                  props.waiting(project.id)
                    ? `${project.name}, a chat is waiting on you`
                    : project.name
                }
                title={project.name}
                onClick={() => {
                  props.onOpenProject(project.id);
                }}
              >
                {initials(project.name)}
                <Show when={props.waiting(project.id)}>
                  <span class="mark waiting rail-dot" />
                </Show>
              </button>
            </li>
          )}
        </For>
      </ul>
      {props.extra}
    </nav>
  );
}
