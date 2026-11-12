// Discover layout editor on the Settings page: add, remove and reorder pages and lists, then
// save the whole layout with PUT api/settings/layout (the API keeps the order it receives).
(() => {
  const editor = document.getElementById("layout-editor");
  if (!editor) return;

  const apiKey = document.querySelector('meta[name="fetcherr-api-key"]')?.content ?? "";
  const field = (element, name) => element.querySelector(`:scope > .layout-row [data-field="${name}"], :scope > .layout-filters [data-field="${name}"]`)?.value.trim() ?? "";
  const ids = (text) => text.split(",").map((part) => part.trim()).filter(Boolean).map(Number).filter(Number.isInteger);
  const number = (text) => (text === "" ? undefined : Number(text));
  const text = (value) => value || undefined;
  const fromTemplate = (id) => document.getElementById(id).content.firstElementChild.cloneNode(true);

  const toast = (message, kind = "success") => {
    const element = document.createElement("div");
    element.className = `toast ${kind}`;
    element.textContent = message;
    document.getElementById("toasts")?.append(element);
    setTimeout(() => element.remove(), 9000);
  };

  const source = (list) => {
    const kind = field(list, "kind");
    const media = field(list, "media");
    if (kind === "trending") return { kind, media: media || null, window: field(list, "window") || "week" };
    if (["popular", "top_rated", "discover"].includes(kind)) return { kind, media: media || "movie" };
    return { kind };
  };

  const filters = (list) => ({
    genres: ids(field(list, "genres")),
    year_from: number(field(list, "year_from")),
    year_to: number(field(list, "year_to")),
    min_vote: number(field(list, "min_vote")),
    min_votes: number(field(list, "min_votes")),
    sort_by: text(field(list, "sort_by")),
    original_language: text(field(list, "original_language")),
    watch_providers: ids(field(list, "watch_providers")),
    watch_region: text(field(list, "watch_region")),
  });

  const layout = () => ({
    pages: [...editor.querySelectorAll(".layout-page")].map((page) => ({
      id: text(page.dataset.id),
      name: field(page, "name"),
      lists: [...page.querySelectorAll(".layout-list")].map((list) => ({
        id: text(list.dataset.id),
        title: field(list, "title"),
        source: source(list),
        filters: filters(list),
      })),
    })),
  });

  // New pages and lists get their ids from the server; write them back so a second save
  // keeps them stable.
  const adoptIds = (saved) => {
    editor.querySelectorAll(".layout-page").forEach((page, p) => {
      page.dataset.id = saved.pages[p].id;
      page.querySelectorAll(".layout-list").forEach((list, l) => { list.dataset.id = saved.pages[p].lists[l].id; });
    });
  };

  const save = async (button) => {
    button.disabled = true;
    try {
      const response = await fetch("api/settings/layout", {
        method: "PUT",
        headers: { "Content-Type": "application/json", "X-Api-Key": apiKey },
        body: JSON.stringify(layout()),
      });
      const body = await response.json().catch(() => ({}));
      if (!response.ok) throw new Error(body?.error?.message ?? response.statusText);
      adoptIds(body);
      toast("Layout saved");
    } catch (error) {
      toast(error.message, "error");
    } finally {
      button.disabled = false;
    }
  };

  const move = (item, up) => {
    const neighbour = up ? item.previousElementSibling : item.nextElementSibling;
    if (!neighbour || neighbour.tagName === "TEMPLATE") return;
    if (up) neighbour.before(item);
    else neighbour.after(item);
  };

  editor.addEventListener("click", (event) => {
    const button = event.target.closest("[data-action]");
    if (!button) return;
    const item = button.closest(".layout-list") ?? button.closest(".layout-page");
    switch (button.dataset.action) {
      case "up": move(item, true); break;
      case "down": move(item, false); break;
      case "remove": item.remove(); break;
      case "add-list": {
        const list = fromTemplate("layout-list-template");
        button.closest(".layout-page").querySelector(".layout-lists").append(list);
        list.querySelector('[data-field="title"]').focus();
        break;
      }
      case "add-page": {
        const page = fromTemplate("layout-page-template");
        editor.querySelector(".layout-pages").append(page);
        page.querySelector('[data-field="name"]').focus();
        break;
      }
      case "save-layout": save(button); break;
    }
  });
})();
