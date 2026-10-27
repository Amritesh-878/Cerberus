import {
  createMemo,
  createSignal,
  For,
  Match,
  Show,
  Switch,
  untrack,
} from "solid-js";

import { AccountsStep } from "./AccountsStep";
import { preselectedAccount, type Profiles, setDefaults } from "./accounts";
import type { ProgramChoice } from "./pty";
import type { Statuses } from "./statuses";
import { type Tab, tabFor } from "./tab";
import {
  finishBlocker,
  INSTALL_COMMAND,
  installed,
  nextStep,
  preselectedProgram,
  previousStep,
  type Step,
  STEP_TITLES,
  STEPS,
} from "./steps";

interface Props {
  start: Step;
  profiles: Profiles;
  programs: ProgramChoice[];
  statuses: Statuses;
  onProfiles: (profiles: Profiles) => void;
  onOpen: (tab: Tab) => void;
  onRecheckPrograms: () => void;
  onFinish: () => void;
}

export function Walkthrough(props: Props) {
  const [step, setStep] = createSignal(untrack(() => props.start));

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
            programs={props.programs}
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
        <Show when={props.profiles.accounts.length > 0}>
          <button
            type="button"
            class="leave"
            title="Leave without changing the defaults"
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
              Claude Code is not installed. Install it from PowerShell with:
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
              The command is typed for you; press Enter to run it, then close
              the tab. If Claude Code still shows as missing, restart Cerberus
              so it sees the new PATH.
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
  programs: ProgramChoice[];
  onProfiles: (profiles: Profiles) => void;
  onFinish: () => void;
}) {
  const [account, setAccount] = createSignal(
    untrack(() => preselectedAccount(props.profiles)),
  );
  const [program, setProgram] = createSignal(
    untrack(() => preselectedProgram(props.profiles, props.programs)),
  );
  const [cwd, setCwd] = createSignal(
    untrack(() => props.profiles.defaultCwd ?? ""),
  );
  const [error, setError] = createSignal<string>();
  const blocker = createMemo(() =>
    finishBlocker(
      props.profiles,
      { account: account(), program: program(), cwd: cwd() },
      props.programs,
    ),
  );

  const finish = () => {
    const folder = cwd().trim();
    setDefaults({
      account: account(),
      program: program(),
      cwd: folder === "" ? null : folder,
    })
      .then((profiles) => {
        props.onProfiles(profiles);
        props.onFinish();
      })
      .catch((e: unknown) => {
        setError(String(e));
      });
  };

  return (
    <section class="defaults">
      <label>
        Default account
        <select
          onChange={(event) => setAccount(Number(event.currentTarget.value))}
        >
          <For each={props.profiles.accounts}>
            {(a) => (
              <option value={a.id} selected={a.id === account()}>
                {a.name}
              </option>
            )}
          </For>
        </select>
      </label>
      <label>
        Default program
        <select onChange={(event) => setProgram(event.currentTarget.value)}>
          <For each={props.programs.filter((p) => p.program !== null)}>
            {(p) => (
              <option value={p.id} selected={p.id === program()}>
                {p.label}
              </option>
            )}
          </For>
        </select>
      </label>
      <label>
        Start folder
        <input
          placeholder="your home folder"
          value={cwd()}
          onInput={(event) => setCwd(event.currentTarget.value)}
        />
      </label>
      <p>
        <button
          type="button"
          disabled={blocker() !== undefined}
          onClick={finish}
        >
          Finish
        </button>{" "}
        <span class="hint">{blocker()}</span>
      </p>
      <Show when={error()}>
        <p class="error">{error()}</p>
      </Show>
    </section>
  );
}
