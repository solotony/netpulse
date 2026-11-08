import { createRouter, createWebHistory } from "vue-router";
import ConnectionsPage from "../pages/ConnectionsPage.vue";
import PlaceholderPage from "../pages/PlaceholderPage.vue";
import SettingsPage from "../pages/SettingsPage.vue";

export const router = createRouter({
  history: createWebHistory(),
  routes: [
    { path: "/", redirect: "/connections" },
    {
      path: "/connections",
      name: "connections",
      component: ConnectionsPage,
      meta: { title: "Connections" },
    },
    {
      path: "/processes",
      name: "processes",
      component: PlaceholderPage,
      meta: { title: "Processes" },
    },
    {
      path: "/ip-addresses",
      name: "ip-addresses",
      component: PlaceholderPage,
      meta: { title: "IP Addresses" },
    },
    {
      path: "/domains",
      name: "domains",
      component: PlaceholderPage,
      meta: { title: "Domains" },
    },
    {
      path: "/networks",
      name: "networks",
      component: PlaceholderPage,
      meta: { title: "Networks" },
    },
    {
      path: "/statistics",
      name: "statistics",
      component: PlaceholderPage,
      meta: { title: "Statistics" },
    },
    {
      path: "/settings",
      name: "settings",
      component: SettingsPage,
      meta: { title: "Settings" },
    },
  ],
});
