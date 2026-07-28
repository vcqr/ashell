export interface AIMsg {
  type: string;
  payload?: any;
}

export interface AITool {
  name: string;
  command: string;
  description: string;
}


export interface AskUserQuestion {
  title: string;
  items?: string[];
  tips?: string;
}


export interface AIConfirm {
  question: string;
  options?: any;
}