<script lang="ts">
  async function health(): Promise<string> {
    const response = await fetch('/api/health');
    if (!response.ok) {
      throw new Error(`HTTP ${response.status}`);
    }
    return response.text();
  }
</script>

<main>
  <h1><img src="/logo.svg" alt="" />vugent</h1>
  <p>
    Server:
    {#await health()}
      checking…
    {:then status}
      {status}
    {:catch}
      unreachable
    {/await}
  </p>
</main>

<style>
  main {
    padding: 1rem 2rem;
  }

  h1 {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    margin: 0;
  }

  h1 img {
    width: 1.2em;
    height: 1.2em;
  }

  p {
    color: var(--text-muted);
  }
</style>
