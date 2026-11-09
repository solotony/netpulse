import { createRouter, createWebHistory } from "vue-router";
import ConnectionsPage from "../pages/ConnectionsPage.vue";
import DomainsPage from "../pages/DomainsPage.vue";
import IpAddressesPage from "../pages/IpAddressesPage.vue";
import NetworksPage from "../pages/NetworksPage.vue";
import ProbePage from "../pages/ProbePage.vue";
import ProcessesPage from "../pages/ProcessesPage.vue";
import SettingsPage from "../pages/SettingsPage.vue";
import StatisticsPage from "../pages/StatisticsPage.vue";

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
      component: ProcessesPage,
      meta: { title: "Processes" },
    },
    {
      path: "/ip-addresses",
      name: "ip-addresses",
      component: IpAddressesPage,
      meta: { title: "IP Addresses" },
    },
    {
      path: "/domains",
      name: "domains",
      component: DomainsPage,
      meta: { title: "Domains" },
    },
    {
      path: "/networks",
      name: "networks",
      component: NetworksPage,
      meta: { title: "Networks" },
    },
    {
      path: "/statistics",
      name: "statistics",
      component: StatisticsPage,
      meta: { title: "Statistics" },
    },
    {
      path: "/probe",
      name: "probe",
      component: ProbePage,
      meta: { title: "Probe" },
    },
    {
      path: "/settings",
      name: "settings",
      component: SettingsPage,
      meta: { title: "Settings" },
    },
  ],
});
