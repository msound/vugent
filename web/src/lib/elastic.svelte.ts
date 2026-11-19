// The rubber band of ADR 0003 and ADR 0007: scrolling on past an edge pulls something elastic, which springs back
// when the user lets go and snaps once it is pulled far enough. A full-screen page closes this way when scrolled
// past its end, and the detail page opens this way when the header is scrolled into.

import { Spring } from 'svelte/motion';

/** Wheel events closer together than this, in milliseconds, belong to the same gesture. */
const GESTURE_GAP = 150;

export interface ElasticOptions {
  /** Which wheel direction pulls: `down` to scroll on past the bottom, `up` to scroll on past the top. */
  direction: 'down' | 'up';
  /** Whether the edge is reached, so that a gesture starting now pulls. */
  atEdge: () => boolean;
  /** The height of a page, in pixels, for wheels that scroll by pages. */
  pageHeight: () => number;
  /** How far, in pixels of wheel movement past the edge, it must be pulled to snap. */
  snapDistance: number;
  onsnap: () => void;
}

export class Elastic {
  /** How far it is pulled, from 0 to 1. It springs back to 0 when the user lets go. */
  readonly pull = new Spring(0, { stiffness: 0.15, damping: 0.5 });

  #options: ElasticOptions;
  #distance = 0;
  #armed = false;
  #lastWheel = -Infinity;
  #release: ReturnType<typeof setTimeout> | undefined;
  #snapped = false;

  constructor(options: ElasticOptions) {
    this.#options = options;
  }

  /** Handles a wheel event; returns whether the event pulled. */
  onwheel = (event: WheelEvent): boolean => {
    if (this.#snapped) {
      return false;
    }
    const scale =
      event.deltaMode === WheelEvent.DOM_DELTA_LINE
        ? 16
        : event.deltaMode === WheelEvent.DOM_DELTA_PAGE
          ? this.#options.pageHeight()
          : 1;
    const sign = this.#options.direction === 'down' ? 1 : -1;
    const delta = event.deltaY * scale * sign;

    // Only a gesture that starts at the edge pulls. A fast scroll that carries the reader into the edge stops
    // there; they must scroll again, on purpose, to pull.
    if (event.timeStamp - this.#lastWheel > GESTURE_GAP) {
      this.#armed = this.#options.atEdge();
    }
    this.#lastWheel = event.timeStamp;

    if (!this.#armed || delta <= 0) {
      this.#armed = false;
      this.letGo();
      return false;
    }

    this.#distance += delta;
    if (this.#distance >= this.#options.snapDistance) {
      this.#snap();
      return true;
    }
    // The further it is pulled, the harder it pulls back.
    const progress = this.#distance / this.#options.snapDistance;
    void this.pull.set(1 - (1 - progress) ** 2, { instant: true });
    clearTimeout(this.#release);
    this.#release = setTimeout(this.letGo, GESTURE_GAP);
    return true;
  };

  /** Lets it spring back. */
  letGo = () => {
    clearTimeout(this.#release);
    this.#distance = 0;
    this.pull.target = 0;
  };

  /** Makes it ready to be pulled again after it snapped. */
  reset() {
    this.#snapped = false;
    this.letGo();
    void this.pull.set(0, { instant: true });
  }

  /** Stops a pending release; call it when the element that pulls goes away. */
  destroy() {
    clearTimeout(this.#release);
  }

  #snap() {
    clearTimeout(this.#release);
    this.#snapped = true;
    void this.pull.set(1, { instant: true });
    this.#options.onsnap();
  }
}
