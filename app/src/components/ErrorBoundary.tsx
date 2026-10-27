// Catches a crashing page so that only its own route breaks.

import { Component, type ErrorInfo, type ReactNode } from "react";

import { getDict } from "../i18n";
import { Callout } from "./Feedback";
import { TechnicalDetails } from "./Layout";

interface Props {
  /** Resets the boundary when it changes (e.g. the route). */
  resetKey: string;
  children: ReactNode;
}

interface State {
  error: Error | null;
  key: string;
}

/** Shows a calm message instead of a crashed subtree. */
export class ErrorBoundary extends Component<Props, State> {
  override state: State = { error: null, key: this.props.resetKey };

  static getDerivedStateFromError(error: unknown): Partial<State> {
    return { error: error instanceof Error ? error : new Error(String(error)) };
  }

  static getDerivedStateFromProps(props: Props, state: State): Partial<State> | null {
    if (props.resetKey !== state.key) {
      return { error: null, key: props.resetKey };
    }
    return null;
  }

  override componentDidCatch(error: Error, info: ErrorInfo): void {
    console.error("page crashed", error, info.componentStack);
  }

  override render(): ReactNode {
    const { error } = this.state;
    if (error === null) {
      return this.props.children;
    }
    const dict = getDict();
    return (
      <Callout kind="danger" title={dict.common.pageCrashed.title} role="alert">
        <p>{dict.common.pageCrashed.body}</p>
        <TechnicalDetails>{`${error.name}: ${error.message}`}</TechnicalDetails>
      </Callout>
    );
  }
}
