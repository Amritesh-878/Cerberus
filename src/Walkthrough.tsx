import {
  createSignal,
  For,
  Match,
  onMount,
  Show,
  Switch,
  untrack,
} from "solid-js";

import { DefaultAccountPicker } from "./AccountsScreen";
import { AccountsStep } from "./AccountsStep";
import { type Profiles, setSetupStep } from "./accounts";
import { CheckIcon } from "./icons";
import type { ProgramChoice } from "./pty";
import type { Statuses } from "./statuses";
import {
  finishBlocker,
  nextStep,
  otherCliOnly,
  previousStep,
  type Step,
  STEP_HEADINGS,
  STEP_LEADS,
  STEP_TITLES,
  stepDone,
  stepsFor,
} from "./steps";
import { installed, shellChoice, type Tab, tabFor } from "./tab";

interface Props {
  profiles: Profiles;
  programs: ProgramChoice[];
  programsError: unknown;
  statuses: Statuses;
  /** Where the walkthrough opens, so the start page can send the user to what is missing. */
  start?: Step;
  onProfiles: (profiles: Profiles) => void;
  onOpen: (tab: Tab) => void;
  onRecheckPrograms: () => void;
  onFinish: () => void;
}

export function Walkthrough(props: Props) {
  const [chosen, setStep] = createSignal<Step>(
    untrack(() => props.start ?? "install"),
  );
  const steps = () => stepsFor(props.programs);
  // A step setup no longer has, such as Accounts once only another AI CLI is installed, shows the first.
  const step = () => (steps().includes(chosen()) ? chosen() : "install");
  const done = (s: Step) => stepDone(s, props.profiles, props.programs);
  const [saveError, setSaveError] = createSignal<string>();
  const saved = (profiles: Profiles) => {
    setSaveError(undefined);
    props.onProfiles(profiles);
  };
  const failed = (error: unknown) => {
    setSaveError(String(error));
  };
  // Each step is saved as it is reached, so a restart reopens setup there.
  const go = (next: Step) => {
    setStep(next);
    setSetupStep(next).then(saved, failed);
  };
  // Reopened from the start page, setup is saved as running again until Finish or Skip.
  onMount(() => {
    if (props.profiles.setup !== step()) go(step());
  });
  // Setup ends only once the core has it, or a restart would bring it back.
  const leave = () => {
    setSetupStep(null).then((profiles) => {
      saved(profiles);
      props.onFinish();
    }, failed);
  };

  return (
    <main class="walkthrough setup">
      <header class="setup-head">
        <h1>Set up Cerberus</h1>
        <button
          type="button"
          class="link"
          onClick={() => {
            leave();
          }}
        >
          Skip setup
        </button>
      </header>
      <ol class="steps">
        <For each={steps()}>
          {(s, i) => (
            <li classList={{ current: s === step(), done: done(s) }}>
              <button
                type="button"
                aria-current={s === step() ? "step" : undefined}
                onClick={() => {
                  go(s);
                }}
              >
                <span class="step-num">
                  <Show when={done(s)} fallback={i() + 1}>
                    <CheckIcon />
                  </Show>
                </span>
                <span class="step-name">{STEP_TITLES[s]}</span>
              </button>
            </li>
          )}
        </For>
      </ol>
      <section class="step-card">
        <h2>{STEP_HEADINGS[step()]}</h2>
        <p class="step-lead">{STEP_LEADS[step()]}</p>
        <Switch>
          <Match when={step() === "install"}>
            <InstallStep
              profiles={props.profiles}
              programs={props.programs}
              programsError={props.programsError}
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
            <section class="defaults">
              <Show
                when={props.profiles.accounts.length > 0}
                fallback={
                  <p class="hint">
                    No accounts yet. Add one in Accounts, then pick it here.
                  </p>
                }
              >
                <DefaultAccountPicker
                  profiles={props.profiles}
                  onProfiles={props.onProfiles}
                />
              </Show>
            </section>
          </Match>
        </Switch>
      </section>
      <Show when={saveError()}>
        {(error) => <p class="error">Setup progress not saved: {error()}</p>}
      </Show>
      <nav class="step-nav">
        <Show when={previousStep(step(), steps())}>
          {(back) => (
            <button
              type="button"
              onClick={() => {
                go(back());
              }}
            >
              Back
            </button>
          )}
        </Show>
        <span class="spacer" />
        <Show
          when={nextStep(step(), steps())}
          fallback={
            <button
              type="button"
              class="go"
              title={finishBlocker(props.profiles, props.programs)}
              disabled={
                finishBlocker(props.profiles, props.programs) !== undefined
              }
              onClick={() => {
                leave();
              }}
            >
              Finish
            </button>
          }
        >
          {(next) => (
            <button
              type="button"
              class="go"
              onClick={() => {
                go(next());
              }}
            >
              Next
            </button>
          )}
        </Show>
      </nav>
    </main>
  );
}

function InstallStep(props: {
  profiles: Profiles;
  programs: ProgramChoice[];
  programsError: unknown;
  onOpen: (tab: Tab) => void;
  onRecheck: () => void;
}) {
  const claude = () => installed(props.programs, "claude");
  const choice = () => props.programs.find((p) => p.id === "claude");
  const command = () => choice()?.installCommand ?? null;
  const named = () => choice()?.installShell ?? null;
  const shell = () => {
    const id = named();
    return id === null
      ? shellChoice(props.profiles, props.programs)
      : installed(props.programs, id);
  };
  const shellLabel = () => {
    const id = named();
    const listed = props.programs.find((p) => p.id === id)?.label;
    return shell()?.label ?? listed ?? "a terminal";
  };
  const openInstaller = () => {
    const using = shell();
    const input = command();
    const tab =
      using &&
      input !== null &&
      tabFor(using, { account: null, cwd: null, input });
    if (tab) props.onOpen(tab);
  };

  return (
    <section>
      <Show when={props.programsError !== undefined}>
        <p class="error">
          Could not list programs: {String(props.programsError)}
        </p>
      </Show>
      <Show when={otherCliOnly(props.programs)}>
        {(other) => (
          <p class="step-ok">
            <CheckIcon /> {other().label} is installed, so tabs can run it now.
            Claude Code and its accounts are optional.
          </p>
        )}
      </Show>
      <Show
        when={claude()}
        fallback={
          <>
            <p>
              Claude Code isn't installed yet. To install it, run this in{" "}
              {shellLabel()}:
            </p>
            <Show when={choice()?.searched}>
              {(searched) => <p class="hint">{searched()}</p>}
            </Show>
            <pre>{command()}</pre>
            <p class="step-actions">
              <button
                type="button"
                disabled={shell() === undefined || command() === null}
                onClick={openInstaller}
              >
                Open in {shellLabel()}
              </button>
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
              The command is typed for you to run. Close the tab after. Still
              missing? Restart Cerberus.
            </p>
          </>
        }
      >
        <p class="step-ok">
          <CheckIcon /> Claude Code is installed.
        </p>
      </Show>
    </section>
  );
}
