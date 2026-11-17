import Reconciler from "react-reconciler";
import { hostConfig } from "./config";

export const reconciler = (Reconciler as any)(hostConfig);
