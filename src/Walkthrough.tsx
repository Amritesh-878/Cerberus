import { createSignal, For, Match, Show, Switch } from "solid-js";

import { DefaultAccountPicker } from "./AccountsScreen";
import { AccountsStep } from "./AccountsStep";
import type { Profiles } from "./accounts";
import type { ProgramChoice } from "./pty";
import type { Statuses } from "./statuses";
import { installed, type Tab, tabFor } from "./tab";
import {
  finishBlocker,
  INSTALL_COMMAND,
  nextStep,
  previousStep,
  type Step,
  STEP_TITLES,
  STEPS,
} from "./steps";

interface Props {
  profiles: Profiles;
  programs: ProgramChoice[];
  statuses: Statuses;
  onProfiles: (profiles: Profiles) => void;
  onOpen: (tab: Tab) => void;
  onRecheckPrograms: () => void;
  onFinish: () => void;
}

export function Walkthrough(props: Props) {
  const [step, setStep] = createSignal<Step>("install");

  return (
    <main class="walkthrough">
      <h1>Set up Cerberus</h1>
      <ol class="steps">
        <For each={STEPS}>
          {(s) => (
            <li classList={{ current: s === step() }}>{STEP_TITLES[s]}</li>
          )}
        </For>
      </ol>
      <Switch>
        <Match when={step() === "install"}>
          <InstallStep
            programs={props.programs}
            onOpen={props.onOpen}
            onRecheck={props.onRecheckPrograms}
          />
        </Match>
        <Match when={step() === "accounts"}>
          <AccountsStep
            profiles={props.profiles}
            programs={props.programs}
            statuses={props.statuses}
            onProfiles={props.onProfiles}
            onOpen={props.onOpen}
          />
        </Match>
        <Match when={step() === "defaults"}>
          <DefaultsStep
            profiles={props.profiles}
            onProfiles={props.onProfiles}
            onFinish={props.onFinish}
          />
        </Match>
      </Switch>
      <nav class="step-nav">
        <Show when={previousStep(step())}>
          {(back) => (
            <button type="button" onClick={() => setStep(back())}>
              Back
            </button>
          )}
        </Show>
        <Show when={nextStep(step())}>
          {(next) => (
            <button type="button" onClick={() => setStep(next())}>
              Next
            </button>
          )}
        </Show>
        <Show when={props.profiles.accounts.length > 0 && nextStep(step())}>
          <button
            type="button"
            class="leave"
            onClick={() => {
              props.onFinish();
            }}
          >
            Back to launcher
          </button>
        </Show>
      </nav>
    </main>
  );
}

function InstallStep(props: {
  programs: ProgramChoice[];
  onOpen: (tab: Tab) => void;
  onRecheck: () => void;
}) {
  const claude = () => installed(props.programs, "claude");
  const openInstaller = () => {
    const shell = installed(props.programs, "powershell");
    const tab =
      shell &&
      tabFor(shell, { account: null, cwd: null, input: INSTALL_COMMAND });
    if (tab) props.onOpen(tab);
  };

  return (
    <section>
      <Show
        when={claude()?.program}
        fallback={
          <>
            <p>
              Claude Code isn't installed yet. To install it, run this in
              PowerShell:
            </p>
            <pre>{INSTALL_COMMAND}</pre>
            <p>
              <button
                type="button"
                disabled={!installed(props.programs, "powershell")}
                onClick={openInstaller}
              >
                Open in PowerShell
              </button>{" "}
              <button
                type="button"
                onClick={() => {
                  props.onRecheck();
                }}
              >
                Check again
              </button>
            </p>
            <p class="hint">
              Cerberus types the command for you. Press Enter to run it, then
              close the tab. If Claude Code still shows as missing, restart
              Cerberus.
            </p>
          </>
        }
      >
        {(path) => (
          <p>
            Claude Code is installed at <code>{path()}</code>.
          </p>
        )}
      </Show>
    </section>
  );
}

function DefaultsStep(props: {
  profiles: Profiles;
  onProfiles: (profiles: Profiles) => void;
  onFinish: () => void;
}) {
  return (
    <section class="defaults">
      <DefaultAccountPicker
        profiles={props.profiles}
        onProfiles={props.onProfiles}
      />
      <p>
        <button
          type="button"
          disabled={finishBlocker(props.profiles) !== undefined}
          onClick={() => {
            props.onFinish();
          }}
        >
          Finish
        </button>{" "}
        <span class="hint">{finishBlocker(props.profiles)}</span>
      </p>
    </section>
  );
}
