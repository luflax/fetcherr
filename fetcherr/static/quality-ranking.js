// Reorders the quality lists on the Settings page. The form submits the ticked qualities in
// list order, and that order is the preference ranking the API stores.
document.addEventListener("click", (event) => {
  const button = event.target.closest(".quality-ranking [data-move]");
  if (!button) return;
  const item = button.closest("li");
  const up = button.dataset.move === "up";
  const neighbour = up ? item.previousElementSibling : item.nextElementSibling;
  if (!neighbour) return;
  if (up) neighbour.before(item);
  else neighbour.after(item);
  button.focus();
});
