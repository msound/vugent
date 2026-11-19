<script lang="ts">
  // The particles of fairy dust (ADR 0008): small dots in the box's border color that start around its edge,
  // scatter outward and fade. They are drawn inside the box, which must be positioned.

  let { width, height }: { width: number; height: number } = $props();

  /** How many particles a box sheds. */
  const COUNT = 10;

  const size = $derived(Math.max(8, Math.min(width, height) * 0.1));
  const reach = $derived(Math.max(width, height) * 0.25 + size * 4);

  // Evenly spread around the box, each turned a little so the pattern does not look regular.
  const sparks = $derived(
    Array.from({ length: COUNT }, (_, n) => {
      const angle = ((n + 0.35 * Math.sin(n * 12.9898)) / COUNT) * 2 * Math.PI;
      const distance = reach * (0.7 + 0.3 * Math.abs(Math.sin(n * 78.233)));
      return {
        x: 50 + 50 * Math.cos(angle),
        y: 50 + 50 * Math.sin(angle),
        dx: Math.cos(angle) * distance,
        dy: Math.sin(angle) * distance,
        delay: (n % 3) * 40,
      };
    }),
  );
</script>

{#each sparks as spark, n (n)}
  <span
    class="spark"
    aria-hidden="true"
    style:left="{spark.x}%"
    style:top="{spark.y}%"
    style:width="{size}px"
    style:height="{size}px"
    style:--dx="{spark.dx}px"
    style:--dy="{spark.dy}px"
    style:animation-delay="{spark.delay}ms"
  ></span>
{/each}

<style>
  .spark {
    position: absolute;
    border-radius: 50%;
    background: var(--change-border);
    box-shadow: 0 0 4px var(--change-border);
    opacity: 0;
    pointer-events: none;
    animation: spark 750ms ease-out forwards;
  }

  @keyframes spark {
    0% {
      opacity: 0;
      transform: translate(-50%, -50%) scale(0.4);
    }

    15% {
      opacity: 1;
    }

    100% {
      opacity: 0;
      transform: translate(calc(-50% + var(--dx)), calc(-50% + var(--dy)))
        scale(1);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .spark {
      display: none;
    }
  }
</style>
