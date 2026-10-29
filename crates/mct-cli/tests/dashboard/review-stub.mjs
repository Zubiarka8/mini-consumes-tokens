// Stands in for review.js, the React review workspace built from crates/mct-cli/web-ui (tested
// there with Vitest). It records what page-findings.js mounts, so these tests check the page's
// side of the contract: when it mounts, what it passes, and when it updates or unmounts.
export const mounts = [];

export function mountReview(host, props) {
  const mount = { host, props, updates: 0, unmounted: false };
  mounts.push(mount);
  const marker = host.ownerDocument.createElement('div');
  marker.className = 'review-stub';
  marker.textContent = props.item.title;
  host.append(marker);
  return {
    update(next) {
      mount.props = next;
      mount.updates += 1;
    },
    unmount() {
      mount.unmounted = true;
      marker.remove();
    },
  };
}
